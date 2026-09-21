use crate::formatter::mk_options;
use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde_json::Value;
use std::collections::HashSet;

pub struct MD013;

impl Rule for MD013 {
    fn name(&self) -> &'static str {
        "MD013"
    }

    fn description(&self) -> &'static str {
        "Line length"
    }

    fn tags(&self) -> &[&str] {
        &["line_length"]
    }

    #[allow(clippy::cast_possible_truncation)] // serde_json gives u64; values are small config counts
    #[allow(clippy::too_many_lines)] // rule logic requires checking multiple interacting config flags
    #[allow(clippy::similar_names)] // `in_code_block` and `is_code_block` are distinct: one tracks parser state, one is a per-line flag
    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let line_length = config
            .and_then(|c| c.get("line_length"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(120) as usize;

        let heading_line_length = config
            .and_then(|c| c.get("heading_line_length"))
            .and_then(serde_json::Value::as_u64)
            .map_or(80, |v| v as usize);

        let check_code_blocks = config
            .and_then(|c| c.get("code_blocks"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let check_tables = config
            .and_then(|c| c.get("tables"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let check_headings = config
            .and_then(|c| c.get("headings"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let mut violations = Vec::new();

        // Track special lines (headings, code blocks, tables, links/images)
        let mut heading_lines = HashSet::new();
        let mut code_block_lines = HashSet::new();
        let mut table_lines = HashSet::new();
        let mut link_only_lines = HashSet::new();
        let mut html_block_lines = HashSet::new();

        let mut in_code_block = false;
        let mut table_start_offset = None;
        let mut html_block_start_offset = None;

        for (event, range) in parser.parse_with_offsets() {
            let line = parser.offset_to_line(range.start);

            match event {
                Event::Start(Tag::Heading { .. }) => {
                    heading_lines.insert(line);
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                }
                Event::Start(Tag::Table(_)) => {
                    table_start_offset = Some(range.start);
                }
                Event::End(TagEnd::Table) => {
                    if let Some(start_off) = table_start_offset {
                        let start_line = parser.offset_to_line(start_off);
                        let end_line = parser.offset_to_line(range.end);
                        for l in start_line..=end_line {
                            table_lines.insert(l);
                        }
                    }
                    table_start_offset = None;
                }
                Event::Start(Tag::HtmlBlock) => {
                    html_block_start_offset = Some(range.start);
                }
                Event::End(TagEnd::HtmlBlock) => {
                    if let Some(start_off) = html_block_start_offset {
                        let start_line = parser.offset_to_line(start_off);
                        let end_line = parser.offset_to_line(range.end);
                        for l in start_line..=end_line {
                            html_block_lines.insert(l);
                        }
                    }
                    html_block_start_offset = None;
                }
                Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                    // Check if this link/image is the only content on the line
                    if let Some(line_text) = parser.lines().get(line - 1) {
                        let trimmed = line_text.trim();
                        // If the line starts with [ or !, it's likely a link/image only line
                        if trimmed.starts_with('[') || trimmed.starts_with("![") {
                            link_only_lines.insert(line);
                        }
                    }
                }
                Event::Text(_) if in_code_block => {
                    code_block_lines.insert(line);
                }
                _ => {}
            }
        }

        // Check each line
        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            let line_len = line.chars().count();

            let is_heading = heading_lines.contains(&line_number);
            let is_code_block = code_block_lines.contains(&line_number);
            let is_table = table_lines.contains(&line_number);
            let is_link_only = link_only_lines.contains(&line_number);
            let is_html_block = html_block_lines.contains(&line_number);

            // Skip lines that only contain links or images (can't be shortened),
            // raw HTML block content (the formatter passes it through verbatim,
            // with no config flag to opt back in since there is nothing for one
            // to enable), and prose lines the formatter has no way to break.
            // Headings, tables, and code blocks are governed by their own config
            // flags, so an explicit `tables = true` still means "check them".
            let governed_by_own_flag = is_heading || is_code_block || is_table;
            if is_link_only || is_html_block || (!governed_by_own_flag && is_unbreakable(line)) {
                continue;
            }

            // Skip if we shouldn't check this type of line
            if is_heading && !check_headings {
                continue;
            }
            if is_code_block && !check_code_blocks {
                continue;
            }
            if is_table && !check_tables {
                continue;
            }

            // Determine the limit for this line
            let limit = if is_heading {
                heading_line_length
            } else {
                line_length
            };

            if line_len > limit {
                violations.push(Violation {
                    line: line_number,
                    column: Some(limit + 1),
                    rule: self.name().to_owned(),
                    message: format!("Line exceeds maximum length ({line_len} > {limit})"),
                    fix: None,
                });
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

/// Whether the formatter has anywhere to break `line`.
///
/// `mdlint format` only ever breaks at a space in plain prose text, so a line
/// whose only spaces sit inside a code span, inline HTML, or a link
/// destination/title -- or whose only space is the one immediately before an
/// inline HTML tag, which the formatter withdraws to keep the tag off column 0
/// -- cannot be shortened by any means the tool has. Reflow produces such
/// lines deliberately rather than corrupt an atomic construct, so reporting
/// them is noise the reader cannot act on.
///
/// This generalises the older "starts with `[`" check, which missed bare URLs
/// and autolinks. Both are kept: the older one also exempts link-only lines that
/// *do* contain spaces.
///
/// Applies to prose only. Headings, tables, and code blocks have their own
/// config flags and are left to those.
fn is_unbreakable(line: &str) -> bool {
    !has_breakable_space(content_after_block_markers(line).trim_end())
}

/// Whether `content` contains a space the formatter would treat as a real
/// break opportunity, mirroring `formatter::on_text`'s rules exactly: a space
/// counts only when it comes from plain prose text, never from inside a code
/// span (`Event::Code`) or inline HTML (`Event::InlineHtml`), and never as the
/// last character of a text run immediately followed by inline HTML.
fn has_breakable_space(content: &str) -> bool {
    let events: Vec<Event<'_>> = Parser::new_ext(content, mk_options()).collect();
    events.iter().enumerate().any(|(i, event)| {
        let Event::Text(text) = event else {
            return false;
        };
        let next_is_inline_html = matches!(events.get(i + 1), Some(Event::InlineHtml(_)));
        let len = text.chars().count();
        text.chars()
            .enumerate()
            .any(|(pos, ch)| ch == ' ' && !(next_is_inline_html && pos + 1 == len))
    })
}

/// `line` with its indent, blockquote markers, and one list marker removed, so
/// that only the content the formatter could rewrap is considered.
fn content_after_block_markers(line: &str) -> &str {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.trim_start();
    }

    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        if let Some(after) = rest[digits..].strip_prefix(['.', ')'])
            && after.starts_with(' ')
        {
            return after.trim_start();
        }
    } else if let Some(after) = rest.strip_prefix(['-', '*', '+'])
        && after.starts_with(' ')
    {
        return after.trim_start();
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_short_lines() {
        let content = indoc! {"
            Short line
            Another short line
            Still short"};
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_long_line() {
        let content = "This is a very long line that definitely exceeds the default eighty character limit and should be flagged";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:81: MD013 Line exceeds maximum length (105 > 80)"]
        );
    }

    #[test]
    fn test_custom_line_length() {
        let content = "This line is exactly forty characters.";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 30 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:31: MD013 Line exceeds maximum length (38 > 30)"]
        );
    }

    #[test]
    fn test_heading_exception() {
        let content =
            "# This is a very long heading that would normally exceed the line length limit";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "headings": false });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_code_block_check() {
        let content = indoc! {"
            ```
            This is a very long line in a code block that exceeds the maximum allowed character count
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80, "code_blocks": true });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:2:81: MD013 Line exceeds maximum length (89 > 80)"]
        );
    }

    #[test]
    fn test_code_block_ignore() {
        let content = indoc! {"
            ```
            This is a very long line in a code block that exceeds the maximum allowed character count
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "code_blocks": false });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_link_only_line_ignored() {
        // A line containing only a link should not trigger line length check
        let content = "[This is a very long link text](https://github.com/example/repository/with/a/very/long/url/path/that/exceeds/the/limit)";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            violations.len(),
            0,
            "Link-only lines should not trigger MD013"
        );
    }

    #[test]
    fn test_image_only_line_ignored() {
        // A line containing only an image should not trigger line length check
        let content = "![Alt text](https://github.com/example/repository/with/a/very/long/image/url/path/that/exceeds/the/maximum/character/limit)";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            violations.len(),
            0,
            "Image-only lines should not trigger MD013"
        );
    }

    #[test]
    fn test_badge_link_ignored() {
        // Badge links (image inside link) should not trigger line length check
        let content = "[![CI](https://github.com/user/repo/workflows/CI/badge.svg)](https://github.com/user/repo/actions/workflows/ci.yml?query=branch%3Amain)";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 120 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0, "Badge links should not trigger MD013");
    }

    #[test]
    fn test_text_with_link_still_checked() {
        // A line with text AND a link should still be checked
        let content = "Check out this link: [example](https://github.com/example/repository/with/a/very/long/url/path) for more information about the thing";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:81: MD013 Line exceeds maximum length (132 > 80)"],
            "Lines with text and links should still be checked"
        );
    }

    #[test]
    fn test_long_table_line() {
        let content = indoc! {"
            | Col1 | Col2 |
            |------|------|
            | A    | B    |
            | C    | D    |"};
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 10, "tables": true });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:11: MD013 Line exceeds maximum length (15 > 10)",
                "test.md:2:11: MD013 Line exceeds maximum length (15 > 10)",
                "test.md:3:11: MD013 Line exceeds maximum length (15 > 10)",
                "test.md:4:11: MD013 Line exceeds maximum length (15 > 10)",
            ]
        );
    }

    #[test]
    fn test_long_table_line_ignored() {
        let content = indoc! {"
            | Col1 | Col2 |
            |------|------|
            | A    | B    |
            | C    | D    |"};
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 10, "tables": false });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_html_block_line_is_not_flagged() {
        // `mdlint format` passes raw HTML block content through verbatim and
        // never reflows it (same guarantee as code blocks), so a long line
        // inside one is unfixable noise, not a real violation. Found via the
        // `formatted_output_never_flags_md013` proptest property.
        let content =
            "<div>\na very long line of raw html content that exceeds the limit\n</div>\n";
        let parser = MarkdownParser::new(content);
        let config = serde_json::json!({ "line_length": 20 });
        assert_eq!(MD013.check(&parser, Some(&config)).len(), 0);
    }

    #[test]
    fn test_bare_long_url_is_not_flagged() {
        // `mdlint format` puts an overlong URL on its own line rather than break
        // it, so flagging that line would be an unfixable complaint about the
        // formatter's own output.
        let content = format!("https://example.com/{}", "segment/".repeat(20));
        let parser = MarkdownParser::new(&content);
        let config = serde_json::json!({ "line_length": 80 });
        assert_eq!(MD013.check(&parser, Some(&config)).len(), 0);
    }

    #[test]
    fn test_single_long_token_in_a_list_item_is_not_flagged() {
        let content = format!("- https://example.com/{}", "segment/".repeat(20));
        let parser = MarkdownParser::new(&content);
        let config = serde_json::json!({ "line_length": 80 });
        assert_eq!(MD013.check(&parser, Some(&config)).len(), 0);
    }

    #[test]
    fn test_long_line_with_a_space_is_still_flagged() {
        // The exemption must not swallow lines the formatter could have wrapped.
        let content = format!("word {}", "x".repeat(200));
        let parser = MarkdownParser::new(&content);
        let config = serde_json::json!({ "line_length": 80 });
        assert_eq!(MD013.check(&parser, Some(&config)).len(), 1);
    }

    #[test]
    fn test_code_span_with_internal_spaces_is_not_flagged() {
        // The formatter never breaks inside a code span, so a line that is
        // nothing but one -- spaces and all -- has nowhere for `format` to put
        // a break. The old text-scan heuristic saw the internal spaces and
        // called it breakable, so `format` then `check` disagreed forever.
        let content = "`a code span with a great many spaces inside it`";
        let parser = MarkdownParser::new(content);
        let config = serde_json::json!({ "line_length": 40 });
        assert_eq!(MD013.check(&parser, Some(&config)).len(), 0);
    }

    #[test]
    fn test_inline_html_with_internal_spaces_is_not_flagged() {
        // Same as the code-span case, but for an HTML attribute value: the
        // spaces live inside the tag, which the formatter also never breaks.
        let content = r#"<span data-attr="a b c d e f g h i j k">t</span>"#;
        let parser = MarkdownParser::new(content);
        let config = serde_json::json!({ "line_length": 40 });
        assert_eq!(MD013.check(&parser, Some(&config)).len(), 0);
    }

    #[test]
    fn test_space_immediately_before_inline_html_is_not_a_break() {
        // `format` withdraws the break right before an inline HTML tag (it
        // would otherwise put the tag at column 0, where it reparses as a
        // block). A line whose only space sits there is therefore unbreakable
        // even though the text scan would see a space.
        let content = r#"alpha <span data-attr="a b c d e f g h i j k">t</span>"#;
        let parser = MarkdownParser::new(content);
        let config = serde_json::json!({ "line_length": 20 });
        assert_eq!(
            MD013.check(&parser, Some(&config)).len(),
            0,
            "the only space is before the inline HTML and must not count as a break"
        );
    }

    #[test]
    fn test_link_title_with_internal_spaces_is_not_flagged() {
        // The link destination and title are never emitted as text the
        // formatter can rewrap -- they are written verbatim at TagEnd::Link.
        let content = r#"[t](https://example.com "a title with a great many words inside")"#;
        let parser = MarkdownParser::new(content);
        let config = serde_json::json!({ "line_length": 40 });
        assert_eq!(MD013.check(&parser, Some(&config)).len(), 0);
    }
}
