use indoc::indoc;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CodeActionOrCommand, InitializeResult, NumberOrString, PublishDiagnosticsParams, TextEdit,
};
use mdlint::formatter;
use mdlint::server::run_server_with_connection;
use std::thread;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Apply whole-line `TextEdit`s (both ends at character 0) to `content`.
fn apply_edits(content: &str, edits: &[TextEdit]) -> String {
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let mut out = String::new();
    let mut cursor = 0usize;
    for edit in edits {
        let start = (edit.range.start.line as usize).min(lines.len());
        let end = (edit.range.end.line as usize).min(lines.len());
        out.push_str(&lines[cursor..start].concat());
        out.push_str(&edit.new_text);
        cursor = end;
    }
    out.push_str(&lines[cursor..].concat());
    out
}

fn next_message(conn: &Connection) -> Message {
    conn.receiver.recv().expect("expected a message")
}

fn next_response(conn: &Connection) -> Response {
    match next_message(conn) {
        Message::Response(r) => r,
        other => panic!("expected Response, got {other:?}"),
    }
}

fn next_notification(conn: &Connection) -> Notification {
    match next_message(conn) {
        Message::Notification(n) => n,
        other => panic!("expected Notification, got {other:?}"),
    }
}

fn send_request(conn: &Connection, id: i32, method: &str, params: serde_json::Value) {
    conn.sender
        .send(Message::Request(Request {
            id: RequestId::from(id),
            method: method.to_owned(),
            params,
        }))
        .unwrap();
}

fn send_notification(conn: &Connection, method: &str, params: serde_json::Value) {
    conn.sender
        .send(Message::Notification(Notification {
            method: method.to_owned(),
            params,
        }))
        .unwrap();
}

/// Perform the LSP initialize handshake from the client side.
fn initialize(client: &Connection) {
    send_request(
        client,
        1,
        "initialize",
        serde_json::json!({
            "processId": null,
            "capabilities": {},
            "rootUri": null
        }),
    );

    let resp = next_response(client);
    let result: InitializeResult = match resp.response_result {
        Ok(result) => serde_json::from_value(result).unwrap(),
        Err(error) => panic!("initialize error: {error:?}"),
    };
    assert!(result.capabilities.text_document_sync.is_some());

    send_notification(client, "initialized", serde_json::json!({}));
}

fn shutdown(client: &Connection) {
    send_request(client, 999, "shutdown", serde_json::json!(null));
    let resp = next_response(client);
    assert!(
        resp.response_result.is_ok(),
        "shutdown error: {:?}",
        resp.response_result
    );
    send_notification(client, "exit", serde_json::json!(null));
}

// ── test ──────────────────────────────────────────────────────────────────────

#[test]
fn lsp_full_lifecycle() {
    // A fresh, empty temp dir (rather than a fixed `/tmp/test.md` path) pins
    // config discovery to "no config found, use defaults" -- `find_all_configs`
    // walks up from the document's directory, so a stray config anywhere from
    // `/tmp` to `/` would otherwise silently change what this test exercises.
    let dir = tempfile::tempdir().expect("create temp dir");
    let file_path = dir.path().join("test.md");
    let uri = url::Url::from_file_path(&file_path).expect("build file:// uri");

    let (server_conn, client_conn) = Connection::memory();

    let server_thread =
        thread::spawn(move || run_server_with_connection(&server_conn, None).unwrap());

    // 1. Initialize handshake
    initialize(&client_conn);

    // MD022 violation: no blank line between headings.
    let content = indoc! {"
        # Title
        ## Section
    "};

    // 2. didOpen → publishDiagnostics
    send_notification(
        &client_conn,
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": uri.as_str(),
                "languageId": "markdown",
                "version": 1,
                "text": content
            }
        }),
    );

    let notif = next_notification(&client_conn);
    assert_eq!(notif.method, "textDocument/publishDiagnostics");
    let params: PublishDiagnosticsParams =
        serde_json::from_value(notif.params).expect("parse publishDiagnostics");
    assert!(
        !params.diagnostics.is_empty(),
        "expected at least one diagnostic"
    );
    // Verify at least one diagnostic has an MD rule code.
    assert!(
        params.diagnostics.iter().any(|d| {
            matches!(&d.code, Some(NumberOrString::String(code)) if code.starts_with("MD"))
        }),
        "expected diagnostic with MD rule code"
    );
    assert!(
        params
            .diagnostics
            .iter()
            .all(|d| d.severity == Some(lsp_types::DiagnosticSeverity::WARNING)),
        "all diagnostics should be warnings"
    );

    // 3. formatting → TextEdit
    send_request(
        &client_conn,
        2,
        "textDocument/formatting",
        serde_json::json!({
            "textDocument": { "uri": uri.as_str() },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );

    let resp = next_response(&client_conn);
    let edits: Vec<TextEdit> = match resp.response_result {
        Ok(result) => serde_json::from_value(result).unwrap(),
        Err(error) => panic!("formatting error: {error:?}"),
    };
    // Content needs formatting; expect one edit that reproduces the formatted
    // document when applied.
    assert_eq!(edits.len(), 1, "expected one TextEdit");
    assert_eq!(
        apply_edits(content, &edits),
        formatter::format(content),
        "applying the returned edits must yield the formatted document"
    );
    // The only change is a blank line between the two headings, so the edit must
    // not span the whole document.
    assert_eq!(
        edits[0].range.start.line, 1,
        "edit should start at the change"
    );
    assert_eq!(
        edits[0].new_text, "\n",
        "edit should insert only the blank line"
    );

    // 4. codeAction for the line of any fixable violation
    let fixable_line = params
        .diagnostics
        .iter()
        .map(|d| d.range.start.line)
        .next()
        .unwrap_or(0);

    send_request(
        &client_conn,
        3,
        "textDocument/codeAction",
        serde_json::json!({
            "textDocument": { "uri": uri.as_str() },
            "range": {
                "start": { "line": fixable_line, "character": 0 },
                "end":   { "line": fixable_line, "character": 0 }
            },
            "context": { "diagnostics": [] }
        }),
    );

    let resp = next_response(&client_conn);
    // Must parse as a valid array of CodeActionOrCommand.
    let _actions: Vec<CodeActionOrCommand> = match resp.response_result {
        Ok(result) => serde_json::from_value(result).expect("parse codeAction result"),
        Err(error) => panic!("codeAction error: {error:?}"),
    };

    // 5. shutdown + exit → server thread completes without panic
    shutdown(&client_conn);

    server_thread.join().expect("server thread panicked");
}

/// The LSP discovers config from each document's own directory, so a config
/// nested next to the document is picked up even though it lives nowhere
/// near the test process's cwd. `mdlint format` run from a directory above
/// this one would *not* see it -- `find_all_configs` only walks up from its
/// start directory -- which is the documented CLI/LSP discovery divergence.
#[test]
fn formatting_uses_the_config_nearest_the_document() {
    let dir = tempfile::tempdir().expect("create temp dir");
    // width 20 makes a two-word line wrap; the default (120) would not.
    std::fs::write(
        dir.path().join("mdlint.toml"),
        "[rules.MD013]\nline_length = 20\n",
    )
    .expect("write nested config");
    let file_path = dir.path().join("test.md");
    let content = "alpha bravo charlie delta echo foxtrot\n";
    std::fs::write(&file_path, content).expect("write test.md");
    let uri = url::Url::from_file_path(&file_path).expect("build file:// uri");

    let (server_conn, client_conn) = Connection::memory();
    let server_thread =
        thread::spawn(move || run_server_with_connection(&server_conn, None).unwrap());

    initialize(&client_conn);

    send_notification(
        &client_conn,
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": uri.as_str(),
                "languageId": "markdown",
                "version": 1,
                "text": content
            }
        }),
    );
    next_notification(&client_conn);

    send_request(
        &client_conn,
        2,
        "textDocument/formatting",
        serde_json::json!({
            "textDocument": { "uri": uri.as_str() },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );

    let resp = next_response(&client_conn);
    let edits: Vec<TextEdit> = match resp.response_result {
        Ok(result) => serde_json::from_value(result).unwrap(),
        Err(error) => panic!("formatting error: {error:?}"),
    };
    let options = formatter::FormatOptions {
        width: 20,
        ..Default::default()
    };
    assert_eq!(
        apply_edits(content, &edits),
        formatter::format_with(content, &options),
        "formatting must reflow at the width from the config next to the document"
    );

    shutdown(&client_conn);
    server_thread.join().expect("server thread panicked");
}

/// A malformed config must fail the formatting request rather than silently
/// fall back to default settings -- format-on-save would otherwise rewrite
/// the document at the wrong width with no indication anything was wrong.
#[test]
fn formatting_fails_when_the_config_is_malformed() {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(dir.path().join("mdlint.toml"), "[rules.MD013\n")
        .expect("write malformed config");
    let file_path = dir.path().join("test.md");
    std::fs::write(&file_path, "content\n").expect("write test.md");
    let uri = url::Url::from_file_path(&file_path).expect("build file:// uri");

    let (server_conn, client_conn) = Connection::memory();
    let server_thread =
        thread::spawn(move || run_server_with_connection(&server_conn, None).unwrap());

    initialize(&client_conn);

    send_notification(
        &client_conn,
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": uri.as_str(),
                "languageId": "markdown",
                "version": 1,
                "text": "content\n"
            }
        }),
    );
    // didOpen falls back to default config for diagnostics; drain the
    // notification it always publishes before issuing the request below.
    next_notification(&client_conn);

    send_request(
        &client_conn,
        2,
        "textDocument/formatting",
        serde_json::json!({
            "textDocument": { "uri": uri.as_str() },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );

    let resp = next_response(&client_conn);
    assert!(
        resp.response_result.is_err(),
        "formatting must fail when the config cannot be loaded, got: {:?}",
        resp.response_result
    );

    shutdown(&client_conn);
    server_thread.join().expect("server thread panicked");
}
