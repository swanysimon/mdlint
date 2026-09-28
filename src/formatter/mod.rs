use std::fmt::Write as _;

use crate::config::{Config, RuleConfig};
use crate::lint::parse_inline_config;
use std::collections::{HashSet, VecDeque};
use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Format a Markdown document to canonical style.
///
/// Returns the formatted document as a String. The output:
/// - Always ends with exactly one trailing newline (or is empty for empty input)
/// - Has exactly one blank line between top-level block elements
/// - Uses ATX-style headings
/// - Uses `-` for unordered list markers
/// - Uses backtick fences for code blocks
#[must_use]
pub fn format(input: &str) -> String {
    format_with(input, &FormatOptions::default())
}

/// The fill column used when no configuration supplies one.  Matches MD013's
/// own `line_length` default so the two agree out of the box.
pub const DEFAULT_WIDTH: usize = 120;

/// The formatter's configuration: the fill column for paragraph reflow
/// (measured in characters, sourced from `rules.MD013.line_length` so a
/// formatted file can never fail MD013 for something the formatter could
/// fix), and whether reflow runs at all.
#[derive(Debug, Clone, Copy)]
pub struct FormatOptions {
    pub width: usize,
    /// When true, `<!-- mdlint-disable MD013 -->` comments are ignored and every
    /// paragraph is reflowed.  Mirrors the config key of the same name.
    pub no_inline_config: bool,
    /// Whether to reflow at all.  Off unless the `reflow` config key or the
    /// `--reflow` flag turns it on, and off whenever MD013 itself is switched
    /// off: disabling the rule inline already stops reflow, and the config key
    /// is the same switch by another spelling.
    pub reflow: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            no_inline_config: false,
            reflow: false,
        }
    }
}

impl From<&Config> for FormatOptions {
    /// Reflow is opt-in via the top-level `reflow` key.  MD013's
    /// `enabled = false` (or `default_enabled = false` without enabling it)
    /// switches it off along with the lint -- the inline directive and the
    /// config key are the same switch spelled two ways, so
    /// `Config::rule_enabled` is the single source of truth for both.
    fn from(config: &Config) -> Self {
        let width = match config.rules.get("MD013") {
            Some(RuleConfig::Config(params)) => params.get("line_length"),
            _ => None,
        }
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|&width| width > 0)
        .unwrap_or(DEFAULT_WIDTH);
        Self {
            width,
            no_inline_config: config.no_inline_config,
            reflow: config.reflow && config.rule_enabled("MD013"),
        }
    }
}

/// Format a Markdown document, reflowing paragraphs to `options.width`.
#[must_use]
pub fn format_with(input: &str, options: &FormatOptions) -> String {
    if input.trim().is_empty() {
        return String::new();
    }

    let mut state = FormatterState::new(options.width);
    let suppressed = reflow_suppressed_lines(input, options);
    let line_starts = line_start_offsets(input);
    let events: Vec<(Event<'_>, Range<usize>)> = Parser::new_ext(input, mk_options())
        .into_offset_iter()
        .collect();
    state.tight_lists = tight_lists(events.iter().map(|(event, _)| event));
    state.code_fences = code_fences(events.iter().map(|(event, _)| event));

    // Precompute per-event lookahead: is the *next* event Start(List(None))?
    let lookahead: Vec<bool> = (0..events.len())
        .map(|i| {
            matches!(
                events.get(i + 1).map(|(event, _)| event),
                Some(Event::Start(Tag::List(None)))
            )
        })
        .collect();

    // Precompute the first character of the immediately following Text event, if
    // any.  pulldown-cmark splits a run like `Ⓐ~A` into three Text events; the
    // `_`/`~` flanking check in on_text needs the char *after* the current event
    // to match its cross-event handling of the char *before* (via self.inline).
    // Only an adjacent Text event contributes an alphanumeric neighbour; any other
    // event (emphasis marker, code, break, block end) is a non-alphanumeric
    // boundary, represented as None.
    let next_text_char: Vec<Option<char>> = (0..events.len())
        .map(|i| match events.get(i + 1).map(|(event, _)| event) {
            Some(Event::Text(t)) => t.chars().next(),
            _ => None,
        })
        .collect();

    for (((event, range), next_is_ul), next_char) in
        events.into_iter().zip(lookahead).zip(next_text_char)
    {
        state.reflow_suppressed =
            !options.reflow || suppressed.contains(&line_at(&line_starts, range.start));
        state.next_is_unordered_list = next_is_ul;
        state.next_text_char = next_char;
        state.process(event);
    }

    state.finish()
}

/// Whether each list, in document order of its `Start` event, is tight.
///
/// pulldown-cmark reports looseness only implicitly: items of a loose list wrap
/// their text in `Paragraph`, items of a tight one do not.  The formatter needs
/// the answer before the first item is written, because a blank line anywhere
/// directly inside a tight item would make the list loose on the next pass.
/// The fence for each code block, in document order.
///
/// Three backticks, unless the content has a line that would close a fence
/// that short: a run of backticks at least as long, indented at most three
/// spaces.  Then one longer than the longest such run, or the block would end
/// early and the rest of its content become Markdown.
fn code_fences<'a>(events: impl Iterator<Item = &'a Event<'a>>) -> VecDeque<String> {
    let mut fences = VecDeque::new();
    let mut content: Option<String> = None;
    for event in events {
        match event {
            Event::Start(Tag::CodeBlock(_)) => content = Some(String::new()),
            Event::Text(text) => {
                if let Some(content) = content.as_mut() {
                    content.push_str(text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                let longest = content
                    .take()
                    .unwrap_or_default()
                    .lines()
                    .filter(|line| line.len() - line.trim_start_matches(' ').len() <= 3)
                    .map(|line| {
                        line.trim_start_matches(' ')
                            .chars()
                            .take_while(|&c| c == '`')
                            .count()
                    })
                    .max()
                    .unwrap_or(0);
                fences.push_back("`".repeat(longest.max(2) + 1));
            }
            _ => {}
        }
    }
    fences
}

fn tight_lists<'a>(events: impl Iterator<Item = &'a Event<'a>>) -> VecDeque<bool> {
    let mut tight = VecDeque::new();
    // Index into `tight` of each open list, and whether each open tag is an item.
    let mut open_lists = Vec::new();
    let mut open_is_item = Vec::new();
    for event in events {
        match event {
            Event::Start(tag) => {
                if matches!(tag, Tag::Paragraph)
                    && open_is_item.last() == Some(&true)
                    && let Some(&list) = open_lists.last()
                {
                    tight[list] = false;
                }
                if matches!(tag, Tag::List(_)) {
                    open_lists.push(tight.len());
                    tight.push_back(true);
                }
                open_is_item.push(matches!(tag, Tag::Item));
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::List(_)) {
                    open_lists.pop();
                }
                open_is_item.pop();
            }
            _ => {}
        }
    }
    tight
}

/// Lines on which MD013 is suppressed by an inline comment, and where the
/// formatter must therefore leave the author's line breaks alone.
///
/// Reuses the linter's directive parser so `<!-- mdlint-disable MD013 -->` means
/// the same thing to both halves of the tool.
fn reflow_suppressed_lines(input: &str, options: &FormatOptions) -> HashSet<usize> {
    if options.no_inline_config || !options.reflow {
        return HashSet::new();
    }
    let directives = parse_inline_config(input);
    ["*", "MD013"]
        .iter()
        .filter_map(|rule| directives.get(*rule))
        .flatten()
        .copied()
        .collect()
}

/// Byte offset at which each line starts, for mapping event ranges to lines.
fn line_start_offsets(input: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(input.match_indices('\n').map(|(index, _)| index + 1))
        .collect()
}

/// The 1-indexed line containing `offset`.
fn line_at(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|&start| start <= offset).max(1)
}

/// Whether `event` opens a new block-level container.  Used to reset
/// `block_reflow_suppressed` so suppression state from a prior sibling block
/// (or from lines that opened it but never flushed it, like a skipped
/// whitespace-only paragraph) cannot leak into this one.
fn starts_new_block(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Start(
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::BlockQuote(_)
                | Tag::List(_)
                | Tag::Item
                | Tag::Table(_)
                | Tag::TableHead
                | Tag::TableRow
                | Tag::TableCell
                | Tag::FootnoteDefinition(_)
        ) | Event::Rule
    )
}

pub(crate) fn mk_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
}

/// What an open blockquote contributes to each line inside it.
const BQ_PREFIX: &str = "> ";

#[allow(clippy::struct_excessive_bools)] // each bool is a distinct formatting phase flag
struct FormatterState {
    out: String,
    /// Whether the next block element should be preceded by a blank line.
    needs_blank: bool,

    // List state
    list_depth: usize,
    /// Start number for ordered list at each depth; None = unordered.
    list_starts: Vec<Option<u64>>,
    /// True while the current output line holds a list item's marker and
    /// nothing after it, so the item's first block can continue that line.
    in_tight_item: bool,
    /// Tightness of every list in the document, in order; consumed as each
    /// list opens.  See `tight_lists`.
    tight_lists: VecDeque<bool>,
    /// Tightness of each open list, innermost last.
    list_tight: Vec<bool>,
    /// True when the last block closed was a nested list.  A paragraph right
    /// after one needs a blank line, or it re-parses as a lazy continuation of
    /// the nested list's last item.
    after_nested_list: bool,

    // Output length when each open blockquote started, so an empty one can be
    // recognised at its End event and still emitted.
    bq_open_lengths: Vec<usize>,

    // What each open container contributes to the start of a continued line, in
    // the order the containers were opened.  Order is the whole point: `> - x`
    // continues as `>   x` while `- > x` continues as `  > x`, and concatenating
    // two independent prefixes can only ever get one of them right.
    container_prefix: Vec<String>,

    // Inline content buffer, flushed when a block element closes.
    inline: String,

    // Byte offsets into `inline` at which a line break may be inserted.  Each
    // offset points at a space that a break would replace.  Only `on_text` and
    // `SoftBreak` contribute; everything else (code spans, link syntax, inline
    // HTML, emphasis markers) is atomic, so a break can never land inside it.
    break_offsets: Vec<usize>,

    // Code block state
    in_code_block: bool,
    /// Fence of every code block in the document, in order; consumed as each
    /// opens.  See `code_fences`.
    code_fences: VecDeque<String>,
    /// Fence of the open code block, repeated to close it.
    code_fence: String,

    // Link/image stack: stores (dest_url, title) from Start until End.
    link_stack: Vec<(String, String)>,

    // Set by the outer format() loop before each event: true when the
    // immediately following event is Start(List(None)).  Used to detect
    // two adjacent unordered lists so we can insert a separator.
    next_is_unordered_list: bool,

    // First char of the next Text event, or None if the next event is not Text.
    // Supplies cross-event right-flank context for the `_`/`~` escape check.
    next_text_char: Option<char>,

    // Fill column for paragraph reflow, in characters.
    width: usize,

    // Set per event by the outer loop: true when this event sits on a line where
    // an inline comment has switched MD013 off.
    reflow_suppressed: bool,

    // Sticky version of the above for the block being accumulated.  Reflow
    // rewrites a whole paragraph or nothing, so a directive covering any part of
    // one protects all of it.  Cleared when the block is flushed.
    block_reflow_suppressed: bool,

    // Table state
    table_alignments: Vec<Alignment>,
    table_head_cells: Vec<String>,
    table_data_rows: Vec<Vec<String>>,
    current_row_cells: Vec<String>,
    in_table_head: bool,
}

impl FormatterState {
    fn new(width: usize) -> Self {
        Self {
            out: String::new(),
            width,
            reflow_suppressed: false,
            block_reflow_suppressed: false,
            needs_blank: false,
            list_depth: 0,
            list_starts: Vec::new(),
            in_tight_item: false,
            after_nested_list: false,
            tight_lists: VecDeque::new(),
            list_tight: Vec::new(),
            bq_open_lengths: Vec::new(),
            container_prefix: Vec::new(),
            inline: String::new(),
            break_offsets: Vec::new(),
            in_code_block: false,
            code_fences: VecDeque::new(),
            code_fence: String::new(),
            link_stack: Vec::new(),
            next_is_unordered_list: false,
            next_text_char: None,
            table_alignments: Vec::new(),
            table_head_cells: Vec::new(),
            table_data_rows: Vec::new(),
            current_row_cells: Vec::new(),
            in_table_head: false,
        }
    }

    fn process(&mut self, event: Event<'_>) {
        // `block_reflow_suppressed` accumulates via `|=` for the lifetime of
        // whatever block is currently being built, so a directive covering any
        // line of it protects the whole thing.  Without a reset at each new
        // block's start, that accumulation carries across block boundaries too
        // -- a suppressed code fence, HTML block, or table left the flag set for
        // the next paragraph, pinning its width at `usize::MAX`.
        if starts_new_block(&event) {
            self.block_reflow_suppressed = false;
        }
        self.block_reflow_suppressed |= self.reflow_suppressed;
        match event {
            Event::Start(tag) => self.on_start(tag),
            Event::End(tag) => self.on_end(tag),
            Event::Text(t) => self.on_text(&t),
            Event::Code(c) => self.emit_inline_code(&c),
            Event::Html(h) => {
                // The block is opaque, but every line of it still has to carry
                // the markers of whatever contains it.  Writing it raw let an
                // HTML block inside a blockquote or list item escape its
                // container entirely.
                for line in h.split_inclusive('\n') {
                    self.open_line();
                    self.out.push_str(line);
                }
                self.in_tight_item = false;
            }
            Event::InlineHtml(h) => {
                // Breaking right before inline HTML would start a line with a
                // tag, where CommonMark reads it as an HTML block.  The emitter
                // would then escape it, turning the tag into literal text and
                // changing the document.  Withdraw the break opportunity so the
                // tag can never land at column 0.
                if let Some(preceding) = self.inline.len().checked_sub(1)
                    && self.break_offsets.last() == Some(&preceding)
                {
                    self.break_offsets.pop();
                }
                self.inline.push_str(&h);
            }
            Event::SoftBreak if self.reflow_suppressed => {
                // Reflow is switched off here, so the author's line break is
                // deliberate: keep it exactly where they put it.
                self.inline.push('\n');
            }
            Event::SoftBreak
                // A soft break renders as a space and carries no meaning of its
                // own, so it is recorded as a break opportunity like any other
                // space and the wrapper decides where the line actually ends.
                if is_space_significant(self.inline.chars().next_back()) => {
                    self.break_offsets.push(self.inline.len());
                    self.inline.push(' ');
                }
            Event::HardBreak => {
                // Backslash + newline = hard line break in CommonMark.
                // Using backslash style avoids trailing-whitespace stripping.
                self.inline.push_str("\\\n");
            }
            Event::Rule => {
                self.flush_pending_inline();
                if self.on_item_marker_line() {
                    // `- ---` is itself a thematic break and would swallow the
                    // list item on re-parse, so the rule goes on its own line
                    // inside the item rather than beside the marker.
                    self.out.push('\n');
                } else if !self.in_tight_item {
                    self.emit_blank_if_needed();
                }
                self.in_tight_item = false;
                self.open_line();
                self.out.push_str("---\n");
                self.needs_blank = true;
            }
            Event::FootnoteReference(label) => {
                write!(self.inline, "[^{label}]").expect("writing to String is infallible");
            }
            Event::TaskListMarker(checked) => {
                if checked {
                    self.inline.push_str("[x] ");
                } else {
                    self.inline.push_str("[ ] ");
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)] // exhaustive match over pulldown-cmark Tag variants
    fn on_start(&mut self, tag: Tag<'_>) {
        let after_nested_list = std::mem::take(&mut self.after_nested_list);
        match tag {
            Tag::Paragraph => {
                // An item's first block continues the marker line, so it gets no
                // blank separator.  The container prefix is written when the
                // paragraph is flushed.
                self.flush_pending_inline();
                if after_nested_list {
                    self.needs_blank = true;
                }
                if !self.in_tight_item {
                    self.emit_blank_if_needed();
                }
                self.in_tight_item = false;
            }
            Tag::Heading { .. } => {
                self.flush_pending_inline();
                self.emit_blank_if_needed();
                // The prefix (hashes) is written at End, when we have the level.
            }
            Tag::CodeBlock(kind) => {
                self.flush_pending_inline();
                self.emit_blank_if_needed();
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.into_string().replace('\\', "\\\\"),
                    CodeBlockKind::Indented => String::new(),
                };
                self.in_tight_item = false;
                self.open_line();
                self.code_fence = self
                    .code_fences
                    .pop_front()
                    .unwrap_or_else(|| "```".to_owned());
                self.out.push_str(&self.code_fence);
                self.out.push_str(&lang);
                self.out.push('\n');
                self.in_code_block = true;
            }
            Tag::List(start) => {
                // Any tight-item text before a sublist (e.g. `Item 1` in
                // `- Item 1\n  - Nested`) goes out first.
                self.flush_pending_inline();
                if self.list_depth == 0 {
                    self.emit_blank_if_needed();
                } else {
                    // Nested list: suppress any pending blank line.
                    // A sublist follows its parent item text without a blank line.
                    self.needs_blank = false;
                    // A sublist directly on its parent's marker line gets a line
                    // of its own, so the two markers cannot merge on re-parse.
                    if self.on_item_marker_line() {
                        self.out.push('\n');
                    }
                    self.in_tight_item = false;
                }
                self.list_depth += 1;
                self.list_tight
                    .push(self.tight_lists.pop_front().unwrap_or(false));
                // Ordered lists always start at 1 in canonical form (MD029).
                self.list_starts.push(start.map(|_| 1u64));
            }
            Tag::Item => {
                // Items of a loose list are separated by a blank line; items of
                // a tight one never are, whatever block the previous item ended
                // with, or the list turns loose on the next pass.
                if self.list_tight.last() == Some(&true) {
                    self.needs_blank = false;
                } else {
                    self.emit_blank_if_needed();
                }
                let marker = match self.list_starts.last_mut() {
                    Some(Some(n)) => {
                        let s = format!("{n}. ");
                        *n += 1;
                        s
                    }
                    _ => "- ".to_owned(),
                };
                // The enclosing containers put a nested item at its parent's
                // content column -- 3 under `1. `, 2 under `- `.  Anything less
                // and the re-parser reads it as a sibling list at the outer level.
                self.open_line();
                self.out.push_str(&marker);
                self.container_prefix.push(" ".repeat(marker.len()));
                self.in_tight_item = true;
            }
            Tag::Emphasis => self.inline.push('*'),
            Tag::Strong => self.inline.push_str("**"),
            Tag::Strikethrough => self.inline.push_str("~~"),
            Tag::Link {
                dest_url, title, ..
            } => {
                self.link_stack
                    .push((dest_url.into_string(), title.into_string()));
                self.inline.push('[');
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                self.link_stack
                    .push((dest_url.into_string(), title.into_string()));
                self.inline.push_str("![");
            }
            Tag::HtmlBlock => {
                self.flush_pending_inline();
                self.emit_blank_if_needed();
            }
            Tag::BlockQuote(_) => {
                self.flush_pending_inline();
                self.emit_blank_if_needed();
                self.bq_open_lengths.push(self.out.len());
                self.container_prefix.push(BQ_PREFIX.to_owned());
            }
            Tag::FootnoteDefinition(label) => {
                self.flush_pending_inline();
                self.emit_blank_if_needed();
                // Write the label prefix; body will be flushed inline.
                self.open_line();
                write!(self.out, "[^{label}]: ").expect("writing to String is infallible");
            }
            Tag::Table(alignments) => {
                self.flush_pending_inline();
                self.emit_blank_if_needed();
                self.table_alignments.clone_from(&alignments);
                self.table_head_cells = Vec::new();
                self.table_data_rows = Vec::new();
                self.current_row_cells = Vec::new();
                self.in_table_head = false;
            }
            Tag::TableHead => {
                self.in_table_head = true;
            }
            Tag::TableRow => {
                self.current_row_cells = Vec::new();
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)] // exhaustive match over pulldown-cmark TagEnd variants
    fn on_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let text = std::mem::take(&mut self.inline);
                // pulldown-cmark may emit a paragraph containing only Unicode
                // whitespace (e.g. NEL U+0085) that is not a CommonMark line
                // ending — finish() strips it via trim_end(), leaving an empty
                // line that turns a blockquote into an empty one on re-parse.
                // Skip the emission entirely; invisible content is no content.
                if text.trim().is_empty() {
                    // No flush_inline_text call means no one consumes the
                    // offsets/suppression state gathered for this paragraph;
                    // left in place, they attach to the next paragraph's text
                    // and wrap_segment treats stale offsets as real breaks,
                    // dropping whatever character sits at each one.
                    self.break_offsets.clear();
                    self.block_reflow_suppressed = false;
                } else {
                    let prefix = self.continuation_prefix();
                    self.flush_inline_text(&text, &prefix);
                    self.needs_blank = true;
                }
                self.in_tight_item = false;
            }
            TagEnd::Heading(level) => {
                let text = std::mem::take(&mut self.inline);
                // Headings are never wrapped; drop the opportunities unused.
                self.break_offsets.clear();
                self.block_reflow_suppressed = false;
                let hashes = "#".repeat(level as usize);
                self.open_line();
                // Collapse hard and soft breaks to spaces, then trim.  Trim must
                // come after: a leading break produces a leading space that trim()
                // removes; trimming first would strip a hard-break marker's `\`,
                // leaving it unescaped and breaking idempotency on re-parse.
                let heading_raw = collapse_heading_breaks(&text);
                let heading_text = escape_trailing_hashes(heading_raw.trim());
                writeln!(self.out, "{hashes} {heading_text}")
                    .expect("writing to String is infallible");
                self.in_tight_item = false;
                self.needs_blank = true;
            }
            TagEnd::CodeBlock => {
                // Ensure code block content ends with a newline so the closing
                // fence is never appended to the last content line.
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.open_line();
                let fence = std::mem::take(&mut self.code_fence);
                self.out.push_str(&fence);
                self.out.push('\n');
                self.in_code_block = false;
                self.needs_blank = true;
            }
            TagEnd::List(_) => {
                self.list_depth -= 1;
                self.list_starts.pop();
                self.list_tight.pop();
                self.after_nested_list = self.list_depth > 0;
                if self.list_depth == 0 {
                    self.needs_blank = true;
                    if self.next_is_unordered_list {
                        // Two adjacent unordered lists would merge into one on
                        // re-parse (both normalise to `-`). Insert an invisible
                        // HTML comment to keep them separate.
                        self.emit_blank_if_needed();
                        self.open_line();
                        self.out.push_str("<!---->\n");
                        self.needs_blank = true;
                    }
                }
            }
            TagEnd::Item => {
                // Tight-item text is never wrapped in a Paragraph, so nothing
                // else flushes it -- including text that follows another block
                // in the same item.
                self.flush_pending_inline();
                // An empty item leaves its marker line open.
                if !self.out.is_empty() && !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.in_tight_item = false;
                self.container_prefix.pop();
            }
            TagEnd::Emphasis => self.inline.push('*'),
            TagEnd::Strong => self.inline.push_str("**"),
            TagEnd::Strikethrough => self.inline.push_str("~~"),
            TagEnd::Link | TagEnd::Image => {
                if let Some((dest, title)) = self.link_stack.pop() {
                    if title.is_empty() {
                        write!(self.inline, "]({dest})").expect("writing to String is infallible");
                    } else {
                        write!(self.inline, "]({dest} \"{title}\")")
                            .expect("writing to String is infallible");
                    }
                }
            }
            TagEnd::HtmlBlock => {
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.needs_blank = true;
            }
            TagEnd::BlockQuote(_) => {
                // An empty blockquote still renders as one.  Dropping it silently
                // deletes a block, and can leave two lists adjacent that the
                // quote had been separating.
                if self.bq_open_lengths.pop() == Some(self.out.len()) {
                    self.open_line();
                    self.out.push('\n');
                }
                self.container_prefix.pop();
                self.needs_blank = true;
            }
            TagEnd::FootnoteDefinition => {
                let text = std::mem::take(&mut self.inline);
                let prefix = self.continuation_prefix();
                self.flush_inline_text(&text, &prefix);
                self.needs_blank = true;
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inline);
                // Cell content is never wrapped; drop the opportunities unused.
                self.break_offsets.clear();
                self.block_reflow_suppressed = false;
                self.current_row_cells.push(cell);
            }
            TagEnd::TableHead => {
                // Cells may have been collected either via End(TableRow) inside the head
                // or directly (if no TableRow wrapper was emitted).
                if self.table_head_cells.is_empty() {
                    self.table_head_cells = std::mem::take(&mut self.current_row_cells);
                }
                self.in_table_head = false;
            }
            TagEnd::TableRow => {
                let row = std::mem::take(&mut self.current_row_cells);
                if self.in_table_head {
                    self.table_head_cells = row;
                } else {
                    self.table_data_rows.push(row);
                }
            }
            TagEnd::Table => {
                let head = std::mem::take(&mut self.table_head_cells);
                let rows = std::mem::take(&mut self.table_data_rows);
                let aligns = std::mem::take(&mut self.table_alignments);

                self.in_tight_item = false;

                // Header row
                self.open_line();
                self.out.push_str("| ");
                self.out.push_str(&head.join(" | "));
                self.out.push_str(" |\n");

                // Separator row
                self.open_line();
                self.out.push_str("| ");
                let seps: Vec<&str> = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::Left => ":---",
                        Alignment::Right => "---:",
                        Alignment::Center => ":---:",
                        Alignment::None => "---",
                    })
                    .collect();
                self.out.push_str(&seps.join(" | "));
                self.out.push_str(" |\n");

                // Data rows
                for row in rows {
                    self.open_line();
                    self.out.push_str("| ");
                    self.out.push_str(&row.join(" | "));
                    self.out.push_str(" |\n");
                }

                self.needs_blank = true;
            }
            _ => {}
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.in_code_block {
            // Code block content goes directly to output, with every
            // container's marker or indent re-added (pulldown-cmark strips
            // them) so the re-parser keeps each line inside its container.
            let prefix = self.continuation_prefix();
            for line in text.split_inclusive('\n') {
                self.out.push_str(&prefix);
                self.out.push_str(line);
            }
        } else {
            // `\\` and `` ` `` are resolved unconditionally by pulldown-cmark regardless
            // of context, so always re-escape them.
            //
            // For `_` and `~`, escape only at positions where the character is NOT between
            // two Unicode alphanumeric characters.  Intra-word delimiters (e.g. `x86_64`)
            // can never open or close emphasis per CommonMark Rule 10/17; everything else
            // must be escaped or it may form emphasis/strikethrough on the next parse.
            //
            // pulldown-cmark may split a single logical run into multiple Text events (e.g.
            // `\_0_` → `Text("_0")` + `Text("_")`).  The combined output `_0_` would form
            // emphasis on re-parse.  To catch this, we use the last char already written to
            // `self.inline` as the "preceding character" for the first char of the event.
            //
            // pulldown-cmark occasionally emits bare `\r` characters inside text events
            // (e.g. from heading content that contains `\r` without a following `\n`).
            // Emitting a raw `\r` into output causes it to be treated as a line ending on
            // re-parse (CommonMark spec §2.3), breaking the heading/paragraph structure and
            // therefore idempotency.  Normalise before processing.
            let text = &*text.replace("\r\n", "\n").replace('\r', "\n");
            let prev_inline_char = self.inline.chars().next_back();
            let chars: Vec<char> = text.chars().collect();
            let mut s = String::with_capacity(text.len() + 4);
            // Offsets are relative to the final position of `s` within `inline`,
            // and escaping shifts them, so they must be taken while building `s`
            // rather than derived from `text`.
            let base = self.inline.len();
            let mut breaks = Vec::new();
            for (i, &ch) in chars.iter().enumerate() {
                // Collapse runs of spaces to one.  A break consumes exactly the
                // space it lands on, so a run would leave a stray space at a line
                // edge that `finish` then trims -- changing the text between
                // passes and breaking idempotency.  Runs render as one space
                // anyway, so collapsing them costs nothing.
                if ch == ' ' {
                    if is_space_significant(s.chars().next_back().or(prev_inline_char)) {
                        breaks.push(base + s.len());
                        s.push(' ');
                    }
                    continue;
                }
                match ch {
                    '\\' => s.push_str("\\\\"),
                    '`' => s.push_str("\\`"),
                    // A literal `<` in a Text event (pulldown only emits `<` as text
                    // when it does NOT already open a tag).  Left bare, adjacent text
                    // can reconstruct an autolink or HTML tag on re-parse (e.g.
                    // `<#@a>` → email autolink), changing meaning.  `\<` renders as
                    // `<` and can never start a tag, so escape unconditionally.
                    '<' => s.push_str("\\<"),
                    '_' | '~' => {
                        let prev = if i > 0 {
                            chars.get(i - 1).copied()
                        } else {
                            prev_inline_char
                        };
                        // For the last char of the event, the right neighbour is
                        // the first char of the next Text event (if any) so that a
                        // run split across events (e.g. `Ⓐ~A` → three events) is
                        // judged the same as the merged form on re-parse.
                        let next = chars.get(i + 1).copied().or(if i + 1 == chars.len() {
                            self.next_text_char
                        } else {
                            None
                        });
                        // Only leave bare when flanked by alphanumeric on BOTH sides.
                        if prev.is_some_and(char::is_alphanumeric)
                            && next.is_some_and(char::is_alphanumeric)
                        {
                            s.push(ch);
                        } else {
                            s.push('\\');
                            s.push(ch);
                        }
                    }
                    _ => s.push(ch),
                }
            }
            self.break_offsets.extend(breaks);
            self.inline.push_str(&s);
        }
    }

    fn emit_inline_code(&mut self, code: &str) {
        // Choose a delimiter longer than any backtick run in the content.
        let max_run = code.chars().fold((0usize, 0usize), |(max, cur), ch| {
            if ch == '`' {
                (max.max(cur + 1), cur + 1)
            } else {
                (max, 0)
            }
        });
        let delim = "`".repeat(max_run.0 + 1);
        let needs_space = code.starts_with('`') || code.ends_with('`');
        self.inline.push_str(&delim);
        if needs_space {
            self.inline.push(' ');
        }
        self.inline.push_str(code);
        if needs_space {
            self.inline.push(' ');
        }
        self.inline.push_str(&delim);
    }

    fn emit_blank_if_needed(&mut self) {
        // Directly inside a tight list item, a blank line would make the list
        // loose on the next pass.  Inside a blockquote in the item it carries
        // a `>` and is not blank.
        let in_tight_item = self.list_tight.last() == Some(&true)
            && self.container_prefix.last().is_some_and(|p| p != BQ_PREFIX);
        if self.needs_blank && !self.out.is_empty() && !in_tight_item {
            // Inside a blockquote, the separator line must carry the `>`
            // markers so the parser keeps both blocks in the same quote.
            let prefix = self.continuation_prefix();
            self.out.push_str(prefix.trim_end());
            self.out.push('\n');
        }
        self.needs_blank = false;
    }

    /// Write whatever part of the container prefix the current output line is
    /// still missing.
    ///
    /// Every block opener calls this.  On a fresh line that is the whole
    /// prefix; on a line an enclosing item marker already opened, it is only
    /// the containers opened after that marker -- so `- > x` gets its `>`,
    /// while `> - # x` does not gain a second one.  Each container's entry is
    /// exactly as wide as the marker that opened it, which is what lets the
    /// current column say how many are already on the line.
    fn open_line(&mut self) {
        let column = self.current_column();
        let mut start = 0;
        let mut missing = String::new();
        for part in &self.container_prefix {
            if start >= column {
                missing.push_str(part);
            }
            start += part.chars().count();
        }
        self.out.push_str(&missing);
    }

    /// Whether the current output line ends in a list item's marker, with no
    /// blockquote opened after it.  A blockquote marker in between already
    /// separates whatever comes next from the item marker.
    fn on_item_marker_line(&self) -> bool {
        self.in_tight_item && self.container_prefix.last().is_some_and(|p| p != BQ_PREFIX)
    }

    /// Flush text a tight list item accumulated outside any Paragraph, before
    /// the next block opens.  Left in the buffer it would be glued onto that
    /// block's content, or dropped.
    fn flush_pending_inline(&mut self) {
        if !self.inline.is_empty() {
            let text = std::mem::take(&mut self.inline);
            let prefix = self.continuation_prefix();
            self.flush_inline_text(&text, &prefix);
            self.in_tight_item = false;
        }
    }

    /// Flush inline text to output.
    /// Each line after the first gets `continuation_prefix`; the first gets
    /// whatever part of the container prefix its output line still lacks.
    fn flush_inline_text(&mut self, text: &str, continuation_prefix: &str) {
        let offsets = std::mem::take(&mut self.break_offsets);
        // Strip trailing hard-break markers (`\\\n`) preceded by only whitespace.
        // A `\` before a line ending that is at the end of a block is re-parsed by
        // pulldown-cmark as a literal `\`, not a hard break — so emitting `\\\n` at
        // the end of a paragraph breaks idempotency (the formatter doubles the `\`
        // on the second pass).  A trailing hard break is always a no-op: there is
        // nothing on the "next line" for the break to separate.
        let text = {
            let s = text.trim_end_matches(|c: char| c != '\n' && c.is_whitespace());
            // Strip a trailing hard-break marker only when the backslash run before
            // `\n` is odd: even runs are content pairs (`\\` = literal `\`) and must
            // not be removed.  An odd run = zero or more content pairs + one marker.
            if let Some(stripped) = s.strip_suffix('\n') {
                let run = stripped.chars().rev().take_while(|&c| c == '\\').count();
                if run % 2 == 1 {
                    &stripped[..stripped.len() - 1]
                } else {
                    text
                }
            } else {
                text
            }
        };
        self.open_line();
        let cont_indent = continuation_prefix.chars().count();
        // A protected block keeps whatever line lengths the author chose.
        let width = if self.block_reflow_suppressed {
            usize::MAX
        } else {
            self.width
        };
        self.block_reflow_suppressed = false;
        let cont_width = width.saturating_sub(cont_indent).max(1);

        // Hard breaks split the buffer into segments that must stay separate;
        // each is refilled on its own.
        let mut seg_start = 0usize;
        let segments: Vec<(usize, &str)> = text
            .split('\n')
            .map(|segment| {
                let start = seg_start;
                seg_start += segment.len() + 1;
                (start, segment)
            })
            .collect();
        let last_index = segments.len().saturating_sub(1);
        let mut first_line = true;

        for (index, (start, segment)) in segments.into_iter().enumerate() {
            if !first_line {
                if index == last_index && segment.is_empty() {
                    // Trailing empty string from split: don't emit an extra newline.
                    break;
                }
                // Skip blank or whitespace-only continuation lines.  Inside a paragraph
                // a blank line is impossible in real Markdown (it ends the paragraph).
                // These arise from: (a) consecutive breaks (HardBreak + SoftBreak with
                // no text) whose combined `\n`s produce an empty slot when split; or (b)
                // lines consisting entirely of Unicode whitespace, which finish()'s
                // trim_end() reduces to blank anyway.  Both cases strand any preceding
                // hard-break marker as a literal `\` that on_text doubles on re-parse.
                if segment.trim_end().is_empty() {
                    continue;
                }
            }

            // Break opportunities recorded against `inline`, rebased onto this
            // segment.  An offset at `start` would be a leading space, and one at
            // the segment end is the hard-break newline itself: neither is usable.
            let breaks: Vec<usize> = offsets
                .iter()
                .filter(|&&offset| offset > start && offset < start + segment.len())
                .map(|&offset| offset - start)
                .collect();

            // The opening line continues whatever is already on the output line
            // (a list marker, a footnote label), so its budget is what remains.
            let first_width = if first_line {
                width.saturating_sub(self.current_column()).max(1)
            } else {
                cont_width
            };

            for line in wrap_segment(segment, &breaks, first_width, cont_width, !first_line) {
                if !first_line {
                    self.out.push_str(continuation_prefix);
                }
                if needs_line_escape(line, !first_line) {
                    self.out.push_str(&escape_line(line));
                } else {
                    self.out.push_str(line);
                }
                self.out.push('\n');
                first_line = false;
            }
        }
    }

    /// What a continued line of the current block must start with: every open
    /// container's marker or indent, in the order they were opened.
    fn continuation_prefix(&self) -> String {
        self.container_prefix.concat()
    }

    /// The current (unterminated) output line.
    fn current_line(&self) -> &str {
        self.out
            .rfind('\n')
            .map_or(self.out.as_str(), |index| &self.out[index + 1..])
    }

    /// Number of characters already written on the current output line.
    fn current_column(&self) -> usize {
        self.current_line().chars().count()
    }

    fn finish(mut self) -> String {
        let s = std::mem::take(&mut self.out);
        let mut result: Vec<&str> = Vec::new();
        let mut prev_blank = false;
        for line in s.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                if !prev_blank {
                    result.push(line);
                }
                prev_blank = true;
            } else {
                result.push(line);
                prev_blank = false;
            }
        }
        // Strip leading blank lines (e.g. from Unicode-whitespace-only lines such
        // as NBSP that trim_end() reduces to empty but aren't caught by the
        // initial input.trim().is_empty() guard).
        let start = result
            .iter()
            .position(|l| !l.is_empty())
            .unwrap_or(result.len());
        let joined = result
            .get(start..)
            .expect("start bounded by result.len()")
            .join("\n");
        let trimmed = joined.trim_end_matches('\n');
        if trimmed.is_empty() {
            return String::new();
        }
        format!("{trimmed}\n")
    }
}

/// Collapse the break markers inside a heading's inline buffer to single spaces.
///
/// A heading cannot span lines, so both soft breaks (`\n`) and hard breaks
/// (`\` + `\n`, emitted by `on_start`/`HardBreak`) become spaces.  The subtlety
/// is telling a hard-break `\` apart from a literal backslash in the heading
/// text: `on_text` always doubles content backslashes, so a run of backslashes
/// originating from content is even-length.  A hard break adds exactly one more,
/// making the run before the newline odd.  We therefore keep floor(n/2) escaped
/// backslashes (the content) and drop the trailing odd one (the marker) before
/// replacing the newline with a space.
fn collapse_heading_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let mut run = 1usize;
            while chars.peek() == Some(&'\\') {
                chars.next();
                run += 1;
            }
            // An odd run immediately before a newline ends in a hard-break marker;
            // emit the content pairs and drop the marker backslash.
            let is_hard_break = run % 2 == 1 && chars.peek() == Some(&'\n');
            let content_backslashes = if is_hard_break { run - 1 } else { run };
            for _ in 0..content_backslashes {
                out.push('\\');
            }
        } else if ch == '\n' {
            // The text before a break may already end in a space, and doubling it
            // here would survive into the heading only to be collapsed by on_text
            // on the next pass.
            if is_space_significant(out.chars().next_back()) {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Escape a trailing run of `#` in heading text.
///
/// `## text #` is a *closed* ATX heading: the trailing run is a closing sequence
/// and is dropped on re-parse, losing a character each pass.  A backslash before
/// the run makes it literal text, and `\#` parses back to `#`, so the escape is
/// re-derived identically on every pass.
fn escape_trailing_hashes(text: &str) -> String {
    let run = text.chars().rev().take_while(|&ch| ch == '#').count();
    if run == 0 {
        return text.to_owned();
    }
    let split = text.len() - run;
    format!("{}\\{}", &text[..split], &text[split..])
}

/// Whether a space following `prev` carries any meaning.
///
/// A space is dropped when it would double an existing one or sit at the start
/// of a line (after a hard break, or at the very start of the block).  Both are
/// invisible when rendered, and both would otherwise leave a stray space at a
/// line edge for `finish` to trim -- changing the text between passes.
fn is_space_significant(prev: Option<char>) -> bool {
    !matches!(prev, None | Some(' ' | '\n'))
}

/// Greedily break `segment` into lines fitting the given character budgets,
/// breaking only at the ascending byte offsets in `breaks` (each a space that
/// the break replaces). `first_line_is_continuation` tells the budget
/// reservation below which escaping rule applies to the very first line this
/// call produces; every line after that is always a continuation.
///
/// A token longer than the budget is never split: the line overflows instead.
/// Splitting it would corrupt exactly the things that get long -- URLs, paths,
/// identifiers -- so an over-width line is the lesser harm.
fn wrap_segment<'a>(
    segment: &'a str,
    breaks: &[usize],
    first_width: usize,
    cont_width: usize,
    first_line_is_continuation: bool,
) -> Vec<&'a str> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut width = first_width;
    let mut is_continuation = first_line_is_continuation;

    loop {
        let mut end = line_end(segment, breaks, start, width, cont_width);
        // `escape_line`'s backslash would push a line filled right up to
        // `width` one character past it, and MD013 would flag a line `format`
        // can never fix.  Whether a line needs the escape depends on the whole
        // line, not its first token -- `** **` is a thematic break, `**` is
        // not -- so the check runs on the line actually chosen.
        let line = &segment[start..end];
        if line.chars().count() >= width && needs_line_escape(line, is_continuation) {
            end = line_end(
                segment,
                breaks,
                start,
                width.saturating_sub(1).max(1),
                cont_width,
            );
        }
        lines.push(&segment[start..end]);
        if end >= segment.len() {
            return lines;
        }
        start = end + 1;
        width = cont_width;
        is_continuation = true;
        if start >= segment.len() {
            return lines;
        }
    }
}

/// Where the line starting at `start` ends: the whole rest of the segment if it
/// fits in `width` or cannot be broken, otherwise the break chosen for it.
fn line_end(
    segment: &str,
    breaks: &[usize],
    start: usize,
    width: usize,
    cont_width: usize,
) -> usize {
    if segment[start..].chars().count() <= width {
        return segment.len();
    }
    next_break(segment, breaks, start, width).map_or(segment.len(), |chosen| {
        retreat_past_structure(segment, breaks, start, chosen, cont_width)
    })
}

/// The break to use for a line starting at `start` with `width` characters
/// available: the furthest one that still fits, or -- when even the first word
/// overflows -- the first one past the budget.
fn next_break(segment: &str, breaks: &[usize], start: usize, width: usize) -> Option<usize> {
    breaks
        .iter()
        .copied()
        .filter(|&offset| offset > start)
        .take_while(|&offset| segment[start..offset].chars().count() <= width)
        .last()
        .or_else(|| breaks.iter().copied().find(|&offset| offset > start))
}

/// Pull `chosen` back to an earlier break when the line it would open re-parses
/// as a block element -- a list marker, an ATX heading, a fence.
///
/// This is cosmetic only. The emitter escapes any structural line regardless, so
/// correctness never rests on the retreat succeeding; it just keeps backslashes
/// out of the output when there is an earlier place to break. When there is not,
/// `chosen` comes back unchanged and the escape does its job.
fn retreat_past_structure(
    segment: &str,
    breaks: &[usize],
    start: usize,
    chosen: usize,
    cont_width: usize,
) -> usize {
    let mut candidate = chosen;
    loop {
        let end = next_break(segment, breaks, candidate + 1, cont_width).unwrap_or(segment.len());
        if !needs_line_escape(&segment[candidate + 1..end], true) {
            return candidate;
        }
        // Strictly decreasing, so this terminates.
        match breaks
            .iter()
            .copied()
            .rev()
            .find(|&offset| offset > start && offset < candidate)
        {
            Some(earlier) => candidate = earlier,
            None => return chosen,
        }
    }
}

/// Escape `line` so that it round-trips through pulldown-cmark as plain text.
///
/// All block-trigger patterns whose first character is ASCII punctuation are
/// escaped by prepending `\`.  The exception is ordered-list markers (`0.`,
/// `12.`): digits are not ASCII punctuation, so `\0` is not a valid `CommonMark`
/// escape and would be doubled on re-parse.  Instead we place the backslash
/// before the `.` or `)` — `0\.` — which is valid and renders identically.
fn escape_line(line: &str) -> String {
    let digits_len = line.chars().take_while(char::is_ascii_digit).count();
    if digits_len > 0 {
        format!("{}\\{}", &line[..digits_len], &line[digits_len..])
    } else {
        format!("\\{line}")
    }
}

/// Returns true if `line`, when emitted as the start of a new output line,
/// would be re-interpreted as a structural block element on re-parse.
///
/// Uses pulldown-cmark itself as the oracle: if parsing `line` in isolation
/// does not produce `Start(Paragraph)` as its first event, the line will be
/// misread — escape it.  This delegates all structural detection to the same
/// parser that the formatter and linter use, so new `CommonMark` edge cases are
/// handled automatically without manual pattern maintenance.
///
/// The one exception kept as a manual check is the setext heading underline on
/// a continuation line (`===`, `--` etc.): these parse as plain paragraphs in
/// isolation but turn the *preceding* output line into a heading when emitted
/// together.  That context-sensitivity cannot be detected by a single-line parse.
///
/// On continuation lines (`is_continuation = true`), ordered-list markers other
/// than `1.`/`1)` do NOT interrupt a paragraph (`CommonMark` spec §5.2) and must
/// not be escaped — escaping them hides broken-list errors from the linter.
/// Since cmark parses `2. foo` in isolation as a list item, we suppress the
/// escape for those cases here.
fn needs_line_escape(line: &str, is_continuation: bool) -> bool {
    // finish() strips trailing Unicode whitespace; check the trimmed form so
    // structural patterns hidden behind trailing Unicode whitespace are caught.
    let line = line.trim_end();
    if line.is_empty() {
        return false;
    }

    // Setext heading underlines are context-sensitive: `===` / `--` alone parse
    // as paragraphs, but after a text line they become headings.
    if is_continuation {
        let trimmed = line.trim_end_matches([' ', '\t']);
        if !trimmed.is_empty()
            && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'))
        {
            return true;
        }
    }

    // On continuation lines, only `1.`/`1)` can interrupt a paragraph — don't
    // escape other ordered-list numbers even though cmark would flag them.
    if is_continuation {
        let digits_len = line.chars().take_while(char::is_ascii_digit).count();
        if digits_len > 0 {
            let rest = &line[digits_len..];
            if let Some(after) = rest.strip_prefix(['.', ')'])
                && (after.is_empty() || after.starts_with([' ', '\t']))
                && &line[..digits_len] != "1"
            {
                return false;
            }
        }
    }

    // Delegate all other structural detection to cmark: if the line does not
    // parse as a paragraph in isolation, it must be escaped.
    !matches!(
        Parser::new_ext(line, mk_options()).next(),
        Some(Event::Start(Tag::Paragraph))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    /// These tests predate reflow being opt-in and many of them exercise it,
    /// so they run with it on; the default-off path is covered end to end in
    /// `tests/formatter.rs`.
    fn format(input: &str) -> String {
        format_with(
            input,
            &FormatOptions {
                reflow: true,
                ..FormatOptions::default()
            },
        )
    }

    /// Assert that `input` formats to `expected` AND that `expected` is already
    /// canonical (formatting it again produces no change — the "not-fix" side).
    fn assert_formats_to(input: &str, expected: &str) {
        let got = format(input);
        assert_eq!(
            got, expected,
            "format(input) did not match expected.\nInput:\n{input}\nExpected:\n{expected}\nGot:\n{got}"
        );
        assert_eq!(
            format(expected),
            expected,
            "format(expected) != expected — already-canonical content must be unchanged.\nExpected:\n{expected}"
        );
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(format(""), "");
        assert_eq!(format("   "), "");
        assert_eq!(format("\n\n"), "");
    }

    #[test]
    fn test_simple_paragraph() {
        assert_eq!(format("Hello, world."), "Hello, world.\n");
    }

    #[test]
    fn test_atx_heading() {
        assert_eq!(format("# Heading 1"), "# Heading 1\n");
        assert_eq!(format("## Heading 2"), "## Heading 2\n");
        assert_eq!(format("###### Heading 6"), "###### Heading 6\n");
    }

    #[test]
    fn test_heading_and_paragraph() {
        let input = indoc! {"
            # Title

            Some text."};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                # Title

                Some text.
            "}
        );
    }

    #[test]
    fn test_multiple_paragraphs() {
        let input = indoc! {"
            First paragraph.

            Second paragraph."};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                First paragraph.

                Second paragraph.
            "}
        );
    }

    #[test]
    fn test_fenced_code_block() {
        let input = indoc! {"
            ```rust
            let x = 1;
            ```"};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                ```rust
                let x = 1;
                ```
            "}
        );
    }

    #[test]
    fn test_code_block_no_lang() {
        let input = indoc! {"
            ```
            code here
            ```"};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                ```
                code here
                ```
            "}
        );
    }

    #[test]
    fn test_unordered_list() {
        let input = indoc! {"
            - Item 1
            - Item 2
            - Item 3"};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                - Item 1
                - Item 2
                - Item 3
            "}
        );
    }

    #[test]
    fn test_ordered_list() {
        let input = indoc! {"
            1. First
            2. Second
            3. Third"};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                1. First
                2. Second
                3. Third
            "}
        );
    }

    #[test]
    fn test_ordered_list_all_ones_renumbered() {
        // "one" style (1. / 1. / 1.) is canonicalized to sequential.
        assert_formats_to(
            indoc! {"
                1. First
                1. Second
                1. Third"},
            indoc! {"
                1. First
                2. Second
                3. Third
            "},
        );
    }

    #[test]
    fn test_ordered_list_non_one_start_renumbered() {
        // Lists starting at a number other than 1 are renumbered from 1.
        assert_formats_to(
            indoc! {"
                3. First
                5. Second
                9. Third"},
            indoc! {"
                1. First
                2. Second
                3. Third
            "},
        );
    }

    #[test]
    fn test_bold_italic_inline() {
        assert_eq!(format("**bold** and *italic*"), "**bold** and *italic*\n");
    }

    #[test]
    fn test_inline_code() {
        assert_eq!(format("Use `foo()` here."), "Use `foo()` here.\n");
    }

    #[test]
    fn test_link() {
        let input = "[text](https://example.com)";
        let output = format(input);
        assert_eq!(output, "[text](https://example.com)\n");
    }

    #[test]
    fn test_image() {
        let input = "![alt text](image.png)";
        let output = format(input);
        assert_eq!(output, "![alt text](image.png)\n");
    }

    #[test]
    fn test_blank_line_between_heading_and_code() {
        let input = indoc! {"
            # Heading

            ```
            code
            ```"};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                # Heading

                ```
                code
                ```
            "}
        );
    }

    #[test]
    fn test_blank_line_between_list_and_paragraph() {
        let input = indoc! {"
            - item

            After list."};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                - item

                After list.
            "}
        );
    }

    #[test]
    fn test_nested_list() {
        let input = indoc! {"
            - Item 1
              - Nested
            - Item 2"};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                - Item 1
                  - Nested
                - Item 2
            "}
        );
    }

    #[test]
    fn test_strikethrough() {
        assert_eq!(format("~~struck~~"), "~~struck~~\n");
    }

    // --- Canonicalization ---

    // Headings: setext → ATX (both levels)
    #[test]
    fn test_setext_headings_to_atx() {
        assert_formats_to("Heading 1\n=========", "# Heading 1\n");
        assert_formats_to("Heading 2\n---------", "## Heading 2\n");
    }

    // Heading with a hard line break in content: the `\` marker must not appear
    // unescaped in output (proptest regression: input "\\\r¡\r=").
    #[test]
    fn test_setext_heading_hard_break_not_leaked() {
        assert_formats_to("\\\r¡\r=", "# ¡\n");
    }

    // Headings: closed ATX → open ATX
    #[test]
    fn test_closed_atx_stripped() {
        assert_formats_to("## Heading ##", "## Heading\n");
        assert_formats_to("# Title #", "# Title\n");
    }

    // Headings: multiple spaces after `#` collapsed to one
    #[test]
    fn test_multiple_spaces_after_hash_collapsed() {
        assert_formats_to("#  Heading", "# Heading\n");
        assert_formats_to("##   Wide", "## Wide\n");
    }

    // Headings: a literal backslash in heading text stays escaped and idempotent.
    // Regression: `#\t0\\\ra` — the `\r` softens to a break, and the collapse of
    // break markers must not consume one of the doubled content backslashes.
    #[test]
    fn test_heading_literal_backslash_idempotent() {
        assert_formats_to("#\t0\\\ra", "# 0\\\\ a\n");
        assert_formats_to("# a\\b", "# a\\\\b\n");
    }

    // A genuine hard break inside a heading collapses to a single space with no
    // stray backslash left behind.
    #[test]
    fn test_heading_hard_break_collapses() {
        assert_formats_to("# a\\\nb", "# a\\\\\n\nb\n");
    }

    // `_`/`~` flanked by alphanumerics across a pulldown-cmark event split must
    // stay bare and idempotent.  Regression: `Ⓐ~A` splits into three Text events;
    // the lone `~` event has no in-event right neighbour, so without cross-event
    // lookahead it escaped on pass 1 then un-escaped on pass 2.
    #[test]
    fn test_intraword_tilde_across_event_split() {
        assert_formats_to("Ⓐ~A", "Ⓐ~A\n");
        // Not flanked on both sides → still escaped.
        assert_formats_to("Ⓐ~", "Ⓐ\\~\n");
        assert_formats_to("~A", "\\~A\n");
    }

    // A literal `<` in text must be escaped so adjacent characters can't
    // reconstruct an autolink or HTML tag on re-parse.  Regression: `<#\@a>`
    // dropped the backslash and re-parsed as an email autolink, changing meaning.
    // Genuine autolinks/HTML arrive as Link/Html events, not Text, so they are
    // unaffected.
    #[test]
    fn test_literal_angle_bracket_escaped() {
        assert_formats_to("<#\\@a>", "\\<#@a>\n");
        assert_formats_to("x<y", "x\\<y\n");
        // Real autolink and inline HTML are preserved, not escaped.
        assert_formats_to(
            "<https://example.com>",
            "[https://example.com](https://example.com)\n",
        );
        assert_formats_to("<div>hi</div>", "<div>hi</div>\n");
    }

    #[test]
    fn test_collapse_heading_breaks_unit() {
        // Soft break → space.
        assert_eq!(collapse_heading_breaks("a\nb"), "a b");
        // Hard-break marker (odd run before newline) dropped; newline → space.
        assert_eq!(collapse_heading_breaks("a\\\nb"), "a b");
        // Doubled content backslash (even run) before a soft break: keep both, then space.
        assert_eq!(collapse_heading_breaks("a\\\\\nb"), "a\\\\ b");
        // Literal backslash not before a newline is untouched.
        assert_eq!(collapse_heading_breaks("a\\\\b"), "a\\\\b");
    }

    // Blank lines: multiple consecutive blank lines collapsed to one
    #[test]
    fn test_multiple_blank_lines_collapsed() {
        assert_formats_to(
            indoc! {"
                First.



                Second."},
            indoc! {"
                First.

                Second.
            "},
        );
    }

    // List markers: * and + → -
    #[test]
    fn test_list_markers_to_dash() {
        assert_formats_to(
            "* Item 1\n* Item 2",
            indoc! {"
                - Item 1
                - Item 2
            "},
        );
        assert_formats_to(
            "+ Item 1\n+ Item 2",
            indoc! {"
                - Item 1
                - Item 2
            "},
        );
    }

    // Emphasis: _ / __ → * / **
    #[test]
    fn test_emphasis_to_asterisk() {
        assert_formats_to("_italic_", "*italic*\n");
        assert_formats_to("__bold__", "**bold**\n");
    }

    // Code fences: ~~~ → ``` (with and without lang tag)
    #[test]
    fn test_tilde_fence_to_backtick() {
        assert_formats_to(
            indoc! {"
                ~~~rust
                code
                ~~~"},
            indoc! {"
                ```rust
                code
                ```
            "},
        );
        assert_formats_to(
            indoc! {"
                ~~~
                code
                ~~~"},
            indoc! {"
                ```
                code
                ```
            "},
        );
    }

    // Horizontal rules: all styles → ---
    #[test]
    fn test_all_hr_styles_to_dashes() {
        assert_formats_to("***", "---\n");
        assert_formats_to("___", "---\n");
        assert_formats_to("* * *", "---\n");
        assert_formats_to("- - -", "---\n");
        assert_formats_to("_ _ _", "---\n");
    }

    // Hard line breaks: trailing-space syntax → backslash continuation.
    // Two spaces before \n must become \\\n so trailing-whitespace stripping
    // doesn't silently drop the line break (CLAUDE.md lessons learned).
    #[test]
    fn test_hard_line_break_becomes_backslash() {
        assert_formats_to("foo  \nbar", "foo\\\nbar\n");
    }

    // Tables
    #[test]
    fn test_simple_table() {
        let input = indoc! {"
            | A | B |
            | --- | --- |
            | 1 | 2 |
            | 3 | 4 |
        "};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                | A | B |
                | --- | --- |
                | 1 | 2 |
                | 3 | 4 |
            "}
        );
    }

    #[test]
    fn test_table_no_leading_pipes() {
        // GFM allows tables without leading/trailing pipes
        assert_formats_to(
            indoc! {"
                A | B
                --- | ---
                1 | 2
            "},
            indoc! {"
                | A | B |
                | --- | --- |
                | 1 | 2 |
            "},
        );
    }

    #[test]
    fn test_table_idempotent() {
        // Proptest uses random strings and is unlikely to generate valid table
        // syntax, so this structural idempotency check is worth keeping explicitly.
        let input = indoc! {"
            | A | B |
            | --- | --- |
            | 1 | 2 |
        "};
        let once = format(input);
        let twice = format(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn test_table_with_inline_formatting() {
        let input = indoc! {"
            | **bold** | `code` |
            | --- | --- |
            | *em* | plain |
        "};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                | **bold** | `code` |
                | --- | --- |
                | *em* | plain |
            "}
        );
    }

    #[test]
    fn test_table_followed_by_paragraph() {
        let input = indoc! {"
            | A | B |
            | --- | --- |
            | 1 | 2 |

            Some text.
        "};
        let output = format(input);
        assert_eq!(
            output,
            indoc! {"
                | A | B |
                | --- | --- |
                | 1 | 2 |

                Some text.
            "}
        );
    }

    // Structural escape: text that starts with a structural character must be
    // escaped so it is not re-interpreted on the next parse pass.
    #[test]
    fn test_escaped_list_marker_in_paragraph() {
        // \* in source resolves to literal *, which must not become a list item
        let once = format("\\*");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: escaped asterisk");
        // Similarly for - and +
        let once = format("\\-");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: escaped dash");
    }

    #[test]
    fn test_setext_heading_with_leading_vt() {
        // VT (U+000B) in setext heading body is preserved by pulldown-cmark, but
        // stripped from ATX heading content on re-parse — trim before emitting.
        let once = format("\u{b}¡\r=");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: setext heading with leading VT");
    }

    #[test]
    fn test_escaped_heading_in_paragraph() {
        // \# in source resolves to literal #, which must not become an ATX heading
        let once = format("\\# not a heading");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: escaped hash");
    }

    // Code blocks inside list items: fences and content must be indented to
    // keep the block inside the list item (3 spaces for `1. `, 2 for `- `).
    #[test]
    fn test_ordered_list_with_code_block() {
        let canonical = indoc! {"
            1. **Enable rule:**

               ```toml
               enabled = false
               ```

            2. **Another item:**

               ```toml
               line_length = 100
               ```
        "};
        // Starting with `1. / 1.` triggers MD029 renumbering in the formatter.
        assert_formats_to(
            indoc! {"
                1. **Enable rule:**

                   ```toml
                   enabled = false
                   ```

                1. **Another item:**

                   ```toml
                   line_length = 100
                   ```
            "},
            canonical,
        );
    }

    #[test]
    fn test_unordered_list_with_code_block() {
        let canonical = indoc! {"
            - **Item:**

              ```toml
              enabled = false
              ```
        "};
        assert_formats_to(canonical, canonical);
    }

    #[test]
    fn test_tight_list_item_code_block_only() {
        // A list item whose sole content is a code block (no text paragraph).
        // The opening fence follows the marker directly; content and closing
        // fence sit at the item's content column so they stay inside it.
        assert_formats_to(
            indoc! {"
                -   ```
                    ¡
                    ```
            "},
            indoc! {"
                - ```
                  ¡
                  ```
            "},
        );
    }

    #[test]
    fn test_tight_code_block_in_nested_item_stays_in_the_item() {
        // The old fence indent was the item's content column *on top of* the
        // marker, so a nested item's fence landed five spaces past its marker
        // and re-parsed as an indented code block.
        let canonical = "- a\n  - ```\n    x\n    ```\n";
        assert_formats_to(canonical, canonical);
    }

    /// Text in a tight item is never wrapped in a Paragraph, so only the end of
    /// the item flushed it.  Any block opening in between was written first and
    /// the text was glued onto that block's content, or lost.
    #[test]
    fn test_tight_item_text_before_a_block_is_flushed_first() {
        for canonical in [
            "- a\n  > b\n",
            "- a\n  # h\n",
            "- a\n  ```\n  x\n  ```\n",
            "- a\n  <div>\n  x\n  </div>\n",
            "- a\n  | b |\n  | --- |\n  | c |\n",
        ] {
            assert_formats_to(canonical, canonical);
        }
    }

    #[test]
    fn test_tight_item_text_after_a_block_is_kept() {
        // The text arrived after the code block had cleared the tight-item
        // flag, so nothing flushed it and it merged into the next item.
        let canonical = "- ```\n  x\n  ```\n  c\n- d\n";
        assert_formats_to(canonical, canonical);
    }

    /// A heading, fence, or table closing a tight item used to request a blank
    /// line before the next item, which made the list loose on the next pass.
    #[test]
    fn test_tight_list_stays_tight_after_a_block_in_an_item() {
        for canonical in [
            "1. a\n2. # b\n3. c\n",
            "- ```\n  x\n  ```\n- d\n",
            "- # h\n  c\n",
        ] {
            assert_formats_to(canonical, canonical);
        }
    }

    /// A nested list's end left no blank line, so a paragraph after it in a
    /// loose item re-parsed as a lazy continuation of the nested list's last
    /// item.
    #[test]
    fn test_paragraph_after_nested_list_in_loose_item_stays_separate() {
        assert_formats_to("- a\n\n  - b\n\n  c\n", "- a\n  - b\n\n  c\n");
        assert_formats_to(
            "> - a\n>\n>   - b\n>\n>   c\n",
            "> - a\n>   - b\n>\n>   c\n",
        );
    }

    #[test]
    fn test_later_blocks_in_a_list_item_keep_the_item_indent() {
        for canonical in [
            "- a\n\n  # h\n",
            "- a\n\n  ---\n",
            "- a\n\n  | b |\n  | --- |\n",
        ] {
            assert_formats_to(canonical, canonical);
        }
    }

    /// Every opener inside a blockquote inside a list item needs both the
    /// item indent and the `>`, in that order.
    #[test]
    fn test_blocks_in_a_blockquote_in_a_list_item_keep_both_prefixes() {
        for canonical in [
            "- > a\n  >\n  > b\n",
            "- > # h\n",
            "- > ```\n  > x\n  > ```\n",
            "- > | a |\n  > | --- |\n  > | b |\n",
            "- > ---\n",
            "- > a\n  >\n  > ---\n",
            "1. > - a\n   >   - b\n",
        ] {
            assert_formats_to(canonical, canonical);
        }
    }

    #[test]
    fn test_adjacent_list_separator_stays_in_its_blockquote() {
        assert_formats_to("> - a\n>\n> * b\n", "> - a\n>\n> <!---->\n>\n> - b\n");
    }

    #[test]
    fn test_setext_underline_in_paragraph_continuation() {
        // "\t=" is stripped to "=" by pulldown-cmark; the bare "=" on a
        // continuation line must be escaped so "a\n=\n" is not re-parsed
        // as a setext h1 heading on the next format pass.
        let once = format("a\r\t=");
        let twice = format(&once);
        assert_eq!(
            once, twice,
            "idempotency: setext-underline-like continuation"
        );
        // Same for "--" which is a valid setext h2 underline.
        let once = format("a\r\t--");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: setext h2 continuation");
    }

    #[test]
    fn test_backtick_in_text_escaped() {
        // A lone backtick in paragraph text must be escaped so it cannot pair
        // with another backtick on re-parse and form an unintended code span.
        let once = format("\\`\r`");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: lone backticks in text");
    }

    #[test]
    fn test_empty_list_items_idempotent() {
        // Two consecutive empty tight items (from "*\r*\t" = two asterisk markers
        // with no content): the old code omitted the newline after each empty item's
        // marker, causing the markers to merge onto one line ("- -") which re-parsed
        // as a nested list on the next pass.
        let once = format("*\r*\t");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: empty tight list items");
    }

    #[test]
    fn test_html_block_with_cr_content_idempotent() {
        // pulldown-cmark splits "<?>\r\" into two Html events within the same
        // HtmlBlock: Html("<?>") and Html("\").  The old Html handler set
        // needs_blank = true after the first event, inserting a spurious blank
        // line before the second, so the second format pass saw "\" as a
        // separate paragraph and escaped it to "\\".
        let once = format("<?>\r\\");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: HTML block with CR content");
    }

    #[test]
    fn test_list_marker_with_trailing_unicode_whitespace_idempotent() {
        // "*\u{85}\u{b}" is paragraph text (NEL and VT are not CommonMark line
        // endings, so `*` has no space after it and is not a list item). But
        // finish() strips trailing Unicode whitespace via trim_end(), leaving
        // bare "*\n" which re-parses as an empty list item on the next pass.
        // The fix: needs_line_escape checks against the trimmed form of the line.
        assert_formats_to("*\u{85}\u{b}", "\\*\n");
    }

    #[test]
    fn test_blockquote_nel_idempotent() {
        // ">\u{85}": cmark emits BlockQuote > Paragraph > Text("\u{85}") — NEL is
        // Unicode whitespace that finish() strips, leaving ">" which re-parses as
        // an empty blockquote → second pass returns "".
        let once = format(">\u{85}");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: blockquote + NEL");
    }

    #[test]
    fn test_trailing_backslash_in_paragraph_not_doubled() {
        // Proptest regression: "¡\\\t\r\u{b}" — pulldown emits Text("¡\") + SoftBreak.
        // on_text doubles the \ to \\; SoftBreak appends \n → inline = "¡\\\n".
        // The trailing-hard-break strip must not fire on an even backslash run (\\
        // = one literal \), only on an odd run (the extra \ is the break marker).
        assert_formats_to("¡\\\t\r\x0B", "¡\\\\\n");
    }

    #[test]
    fn test_hard_break_followed_by_vt_in_paragraph() {
        // "\\\r\u{b}\r¡": cmark emits HardBreak + SoftBreak (VT stripped) + Text("¡").
        // The two consecutive breaks produce an empty continuation slot when split on
        // '\n', which emits a blank line that breaks the paragraph on re-parse — the
        // preceding `\` is then doubled by on_text on the second pass.
        let once = format("\\\r\u{b}\r¡");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: hard-break + VT continuation");
    }

    /// Every fence was written as exactly three backticks, so a block whose
    /// content contains a three-backtick line -- a Markdown example of a code
    /// block -- closed early and the rest of it became Markdown.
    #[test]
    fn test_code_fence_outlasts_backtick_runs_in_content() {
        let canonical = "````markdown\n```toml\nx = 1\n```\n````\n";
        assert_formats_to(canonical, canonical);
        assert_formats_to("~~~\n   ````\n~~~\n", "`````\n   ````\n`````\n");
        // Indented four or more, a run cannot close the fence.
        assert_formats_to("~~~\n    ```\n~~~\n", "```\n    ```\n```\n");
        assert_formats_to("- ````\n  ```\n  ````\n", "- ````\n  ```\n  ````\n");
    }

    #[test]
    fn test_code_fence_info_backslash_idempotent() {
        // pulldown-cmark returns the unescaped info string for fenced code blocks.
        // Emitting it verbatim means "\!" round-trips to "!" (pulldown-cmark
        // treats "\!" as a backslash escape of "!" on the next parse).
        // The fix escapes "\" to "\\" in the info string so the round-trip is stable.
        let once = format("```\\\r!");
        let twice = format(&once);
        assert_eq!(
            once, twice,
            "idempotency: code fence info string with backslash"
        );
    }

    /// `wrap_segment` used to choose a line's word boundary by its *unescaped*
    /// length; a line starting with a block-hazard token that `retreat_past_structure`
    /// had no earlier break to avoid then grew by one backslash from
    /// `escape_line` after the width decision was already made, landing the
    /// emitted line one character past the budget -- an MD013 violation
    /// `format` could never fix. Found by the `formatted_output_never_flags_md013`
    /// proptest property.
    /// `**` alone needs no escape but `** **` is a thematic break, so deciding
    /// the reservation from a line's first token let the escaped line run one
    /// character over.
    #[test]
    fn test_multi_token_hazard_line_stays_within_the_fill_column() {
        let options = FormatOptions {
            width: 5,
            reflow: true,
            ..FormatOptions::default()
        };
        let once = format_with("aaaa ** ** a\n", &options);
        assert_eq!(once, format_with(&once, &options), "idempotency");
        assert!(
            once.lines().all(|line| line.chars().count() <= 5),
            "every line must fit the fill column: {once:?}"
        );
    }

    #[test]
    fn test_escaped_hazard_line_stays_within_the_fill_column() {
        let input = "a \\\n1.\naaa _ | aaaaaaaa aa [ a a a aa aaaaaaa aaaaa aaaaaa aaaaa aa a a\n";
        let options = FormatOptions {
            width: 61,
            reflow: true,
            ..FormatOptions::default()
        };
        let out = format_with(input, &options);
        for line in out.lines() {
            assert!(
                line.chars().count() <= 61,
                "line of {} chars exceeds width 61: {line:?}",
                line.chars().count()
            );
        }
        assert_eq!(out, format_with(&out, &options), "idempotency");
    }

    // ── regressions found by the reflow proptest generator ───────────────────
    // All five predate reflow; the generator's dense structural characters and
    // multi-line paragraphs are what surfaced them.

    #[test]
    fn test_hard_break_in_heading_does_not_double_space() {
        // collapse_heading_breaks turned the break into a space next to the one
        // already ending the text, which on_text then collapsed on pass two.
        let once = format("a a \\\na a\n---\n");
        assert_eq!(
            once,
            format(&once),
            "idempotency: hard break inside heading"
        );
    }

    #[test]
    fn test_heading_text_ending_in_hash_is_escaped() {
        // `## a #` is a closed ATX heading: the trailing `#` is a closing
        // sequence and was dropped on re-parse, losing a character each pass.
        assert_eq!(format("a #\n---\n"), "## a \\#\n");
        let once = format("a #\n---\n");
        assert_eq!(once, format(&once), "idempotency: heading ending in hash");
    }

    #[test]
    fn test_nested_list_under_ordered_item_indents_to_content_column() {
        // Two spaces put the nested list below `1. `'s content column, so it
        // re-parsed as a sibling list at the outer level.
        // The nested list goes on its own line, indented to column 3 -- the
        // content column of `1. ` -- so it stays inside the ordered item.
        let once = format("1. - a\n");
        assert_eq!(once, "1.\n   - a\n");
        assert_eq!(
            once,
            format(&once),
            "idempotency: nested list under ordered"
        );
    }

    #[test]
    fn test_blockquote_inside_list_item_keeps_its_marker() {
        // The first line never got its `>`, so the blockquote was flattened into
        // the item's paragraph and silently disappeared.
        assert_eq!(format("- > alpha bravo\n"), "- > alpha bravo\n");
        let once = format("- > alpha\n  > bravo\n");
        assert_eq!(once, format(&once), "idempotency: blockquote in list item");
    }

    /// A loose list item's *first* block follows the marker on the same line
    /// and needs neither a blank line nor its own indent -- but every block
    /// after that has no marker on its line at all, and without them it fell
    /// to column 0 and merged into the previous block as a lazy continuation
    /// on the next pass. Pre-existing (reproduces on the pre-reflow tip too),
    /// not introduced by reflow.
    #[test]
    fn test_second_paragraph_in_a_list_item_keeps_its_blank_and_indent() {
        let once = format("- first para\n\n  second para\n");
        assert_eq!(
            once, "- first para\n\n  second para\n",
            "second block must stay separate and indented, not merge into the first"
        );
        assert_eq!(
            once,
            format(&once),
            "idempotency: second paragraph in a loose list item"
        );
    }

    #[test]
    fn test_thematic_break_in_tight_list_item_stays_in_the_item() {
        // `- ---` is itself a thematic break, which swallowed the list entirely.
        let once = format("+ ---\n");
        assert_eq!(once, format(&once), "idempotency: rule in tight list item");
        assert!(
            once.starts_with("-\n"),
            "rule must not share the marker line: {once:?}"
        );
    }

    /// `Event::Html` used to write the raw block straight to the output with no
    /// blockquote or list-item prefix, so the container was lost and the document
    /// changed on the next pass.  Last of the container-prefix family.
    #[test]
    fn test_html_block_in_blockquote_keeps_marker() {
        let once = format("> alpha\n> <div> beta\n");
        assert_eq!(once, format(&once), "idempotency: HTML block in blockquote");
        assert!(
            once.lines().all(|line| line.starts_with('>')),
            "every line must stay inside the blockquote: {once:?}"
        );
    }

    #[test]
    fn test_html_block_in_list_item_keeps_its_indent() {
        let once = format("- <div>alpha</div>\n");
        assert_eq!(once, "- <div>alpha</div>\n");
        assert_eq!(once, format(&once), "idempotency: HTML block in list item");
    }

    #[test]
    fn test_empty_blockquote_is_preserved() {
        // Dropping it deletes a block, and can leave two lists adjacent that the
        // quote had been keeping apart.
        let once = format("1. a\n>\n- b\n");
        assert!(
            once.contains("\n>\n"),
            "empty blockquote must survive: {once:?}"
        );
        assert_eq!(once, format(&once), "idempotency: empty blockquote");
    }

    /// A whitespace-only paragraph (e.g. a lone NEL U+0085) is skipped without
    /// emitting output, but its break offsets and suppression flag must still be
    /// cleared -- otherwise they attach to the *next* paragraph's text and
    /// `wrap_segment` drops whatever character sits at each stale offset.
    #[test]
    fn test_whitespace_only_paragraph_does_not_corrupt_the_next_paragraph() {
        let input = "\u{a0} \u{a0} \u{a0} \u{a0} \u{a0}\n\n".to_string() + &"A".repeat(200);
        let out = format(&input);
        assert_eq!(
            out.chars().filter(|&c| c == 'A').count(),
            200,
            "no character may be dropped from the following paragraph: {out:?}"
        );
    }

    #[test]
    fn test_whitespace_only_paragraph_does_not_corrupt_the_next_paragraph_narrow_width() {
        let input = "\u{a0} \u{a0}\n\nalpha bravo charlie\n";
        let out = format_with(
            input,
            &FormatOptions {
                width: 3,
                reflow: true,
                ..FormatOptions::default()
            },
        );
        assert_eq!(
            out, "alpha\nbravo\ncharlie\n",
            "no word may lose a character to a stale break offset: {out:?}"
        );
    }

    /// Block openers used to write only the blockquote marker, not the
    /// enclosing list indent, so a blockquote nested in a list item opened at
    /// column 0 and the list was lost on re-parse.
    #[test]
    fn test_blockquote_nested_in_list_opens_with_the_full_prefix() {
        assert_formats_to("- > - alpha\n", "- > - alpha\n");
        assert_formats_to(
            "- > - alpha\n  >   - bravo\n",
            "- > - alpha\n  >   - bravo\n",
        );
    }
}
