use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CodeActionOrCommand, InitializeResult, NumberOrString, PublishDiagnosticsParams, TextEdit,
};
use mdlint::formatter;
use mdlint::server::run_server_with_connection;
use std::thread;

// ── helpers ───────────────────────────────────────────────────────────────────

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
            method: method.to_string(),
            params,
        }))
        .unwrap();
}

fn send_notification(conn: &Connection, method: &str, params: serde_json::Value) {
    conn.sender
        .send(Message::Notification(Notification {
            method: method.to_string(),
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
    let result: InitializeResult =
        serde_json::from_value(resp.result.expect("initialize result")).unwrap();
    assert!(result.capabilities.text_document_sync.is_some());

    send_notification(client, "initialized", serde_json::json!({}));
}

fn shutdown(client: &Connection) {
    send_request(client, 999, "shutdown", serde_json::json!(null));
    let resp = next_response(client);
    assert!(resp.error.is_none(), "shutdown error: {:?}", resp.error);
    send_notification(client, "exit", serde_json::json!(null));
}

// ── test ──────────────────────────────────────────────────────────────────────

#[test]
fn lsp_full_lifecycle() {
    let (server_conn, client_conn) = Connection::memory();

    let server_thread =
        thread::spawn(move || run_server_with_connection(server_conn, None).unwrap());

    // 1. Initialize handshake
    initialize(&client_conn);

    // MD022 violation: no blank line between headings.
    let content = "# Title\n## Section\n";

    // 2. didOpen → publishDiagnostics
    send_notification(
        &client_conn,
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/test.md",
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
            "textDocument": { "uri": "file:///tmp/test.md" },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );

    let resp = next_response(&client_conn);
    assert!(resp.error.is_none(), "formatting error: {:?}", resp.error);
    let edits: Vec<TextEdit> =
        serde_json::from_value(resp.result.expect("formatting result")).unwrap();
    // Content needs formatting; expect exactly one whole-doc edit.
    assert_eq!(edits.len(), 1, "expected one TextEdit");
    assert_eq!(
        edits[0].new_text,
        formatter::format(content),
        "TextEdit new_text must equal formatter::format(content)"
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
            "textDocument": { "uri": "file:///tmp/test.md" },
            "range": {
                "start": { "line": fixable_line, "character": 0 },
                "end":   { "line": fixable_line, "character": 0 }
            },
            "context": { "diagnostics": [] }
        }),
    );

    let resp = next_response(&client_conn);
    assert!(resp.error.is_none(), "codeAction error: {:?}", resp.error);
    // Must parse as a valid array of CodeActionOrCommand.
    let _actions: Vec<CodeActionOrCommand> =
        serde_json::from_value(resp.result.expect("codeAction result"))
            .expect("parse codeAction result");

    // 5. shutdown + exit → server thread completes without panic
    shutdown(&client_conn);

    server_thread.join().expect("server thread panicked");
}
