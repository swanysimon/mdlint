use crate::config::loader::find_all_configs;
use crate::config::{Config, merge_many_configs};
use crate::error::Result;
use crate::formatter;
use crate::lint::LintEngine;
use crate::server::convert;
use crate::server::documents::DocumentStore;
use crate::types::Violation;
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    PublishDiagnosticsParams, Range, TextEdit, Uri, WorkspaceEdit,
};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn handle_request(conn: &Connection, req: &Request, docs: &mut DocumentStore) {
    match req.method.as_str() {
        "textDocument/formatting" => formatting(conn, req, docs),
        "textDocument/codeAction" => code_action(conn, req, docs),
        _ => send_error(conn, req.id.clone(), -32601, "Method not found"),
    }
}

pub fn handle_notification(conn: &Connection, notif: &Notification, docs: &mut DocumentStore) {
    match notif.method.as_str() {
        "textDocument/didOpen" => did_open(conn, notif, docs),
        "textDocument/didChange" => did_change(conn, notif, docs),
        "textDocument/didClose" => did_close(conn, notif, docs),
        _ => {}
    }
}

fn did_open(conn: &Connection, notif: &Notification, docs: &mut DocumentStore) {
    let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(notif.params.clone())
    else {
        eprintln!("[mdlint-server] Invalid didOpen params");
        return;
    };
    let uri = params.text_document.uri;
    let content = params.text_document.text;
    docs.open(uri.clone(), content.clone());
    let config = load_config(&uri).unwrap_or_else(|err| {
        eprintln!("[mdlint-server] {err}; using default configuration for diagnostics");
        Config::default()
    });
    publish_diagnostics(conn, &uri, &content, config);
}

fn did_change(conn: &Connection, notif: &Notification, docs: &mut DocumentStore) {
    let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(notif.params.clone())
    else {
        eprintln!("[mdlint-server] Invalid didChange params");
        return;
    };
    let uri = params.text_document.uri;
    // Full sync: take the last (and only) content change.
    let Some(change) = params.content_changes.into_iter().last() else {
        return;
    };
    let content = change.text;
    docs.update(&uri, content.clone());
    let config = load_config(&uri).unwrap_or_else(|err| {
        eprintln!("[mdlint-server] {err}; using default configuration for diagnostics");
        Config::default()
    });
    publish_diagnostics(conn, &uri, &content, config);
}

fn did_close(conn: &Connection, notif: &Notification, docs: &mut DocumentStore) {
    let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(notif.params.clone())
    else {
        eprintln!("[mdlint-server] Invalid didClose params");
        return;
    };
    let uri = params.text_document.uri;
    docs.close(&uri);
    // Clear diagnostics for the closed file.
    let empty = PublishDiagnosticsParams {
        uri,
        diagnostics: vec![],
        version: None,
    };
    let _ = conn.sender.send(Message::Notification(Notification::new(
        "textDocument/publishDiagnostics".to_owned(),
        empty,
    )));
}

fn formatting(conn: &Connection, req: &Request, docs: &DocumentStore) {
    let Ok(params) = serde_json::from_value::<DocumentFormattingParams>(req.params.clone()) else {
        send_error(conn, req.id.clone(), -32602, "Invalid params");
        return;
    };
    let uri = &params.text_document.uri;
    let Some(content) = docs.get(uri) else {
        send_error(conn, req.id.clone(), -32602, "Document not found");
        return;
    };
    let content = content.to_owned();
    // Discovered the same way the CLI discovers config (walk up from a
    // directory, closer files win) but from *this document's* directory
    // rather than the CLI's cwd -- without this the server would reflow at
    // the default width while `mdlint format` used the configured one. The
    // two can still disagree for a nested config: `mdlint format` run from
    // the repo root uses the root config for every file, while format-on-save
    // uses whatever config is nearest each document. This is deliberate --
    // each is the natural default for its context -- not a bug to fix here.
    // A config
    // that fails to load must fail the request rather than silently fall back
    // to defaults -- that would rewrite the document at the wrong width with
    // no indication anything was wrong.
    let config = match load_config(uri) {
        Ok(config) => config,
        Err(err) => {
            send_error(conn, req.id.clone(), -32603, &format!("{err}"));
            return;
        }
    };
    let options = formatter::FormatOptions::from(&config);
    let formatted = formatter::format_with(&content, &options);
    let edits: Vec<TextEdit> = convert::minimal_edits(&content, &formatted);
    let resp = Response::new_ok(req.id.clone(), edits);
    let _ = conn.sender.send(Message::Response(resp));
}

fn code_action(conn: &Connection, req: &Request, docs: &DocumentStore) {
    let Ok(params) = serde_json::from_value::<CodeActionParams>(req.params.clone()) else {
        send_error(conn, req.id.clone(), -32602, "Invalid params");
        return;
    };
    let uri = &params.text_document.uri;
    let Some(content) = docs.get(uri) else {
        let resp = Response::new_ok(req.id.clone(), Vec::<CodeActionOrCommand>::new());
        let _ = conn.sender.send(Message::Response(resp));
        return;
    };
    let content = content.to_owned();
    let config = load_config(uri).unwrap_or_else(|err| {
        eprintln!("[mdlint-server] {err}; using default configuration for code actions");
        Config::default()
    });
    let violations = LintEngine::new(config)
        .lint_content(&content)
        .unwrap_or_default();
    let actions = violations_to_actions(uri, &content, &violations, &params.range);
    let resp = Response::new_ok(req.id.clone(), actions);
    let _ = conn.sender.send(Message::Response(resp));
}

// lsp-types requires HashMap<Uri, _> in WorkspaceEdit; Uri has interior mutability by design.
#[allow(clippy::mutable_key_type)]
#[allow(clippy::cast_possible_truncation)] // LSP positions are u32; line counts in real files fit
fn violations_to_actions(
    uri: &Uri,
    content: &str,
    violations: &[Violation],
    range: &Range,
) -> Vec<CodeActionOrCommand> {
    violations
        .iter()
        .filter(|v| {
            v.fix.is_some() && {
                let v_line = v.line.saturating_sub(1) as u32;
                v_line >= range.start.line && v_line <= range.end.line
            }
        })
        .map(|v| {
            let fix = v.fix.as_ref().expect("fix is_some checked above");
            let edit = convert::fix_to_text_edit(fix, content);
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Fix {}", v.rule),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            })
        })
        .collect()
}

pub fn publish_diagnostics(conn: &Connection, uri: &Uri, content: &str, config: Config) {
    let violations = LintEngine::new(config)
        .lint_content(content)
        .unwrap_or_default();
    let diagnostics = violations
        .iter()
        .map(|v| convert::violation_to_diagnostic(v, content))
        .collect();
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };
    let _ = conn.sender.send(Message::Notification(Notification::new(
        "textDocument/publishDiagnostics".to_owned(),
        params,
    )));
}

fn load_config(uri: &Uri) -> Result<Config> {
    let dir = convert::uri_to_path(uri)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let configs = find_all_configs(&dir)?;
    Ok(merge_many_configs(
        configs.into_iter().map(|(_, c)| c).collect(),
    ))
}

fn send_error(conn: &Connection, id: lsp_server::RequestId, code: i32, message: &str) {
    let resp = Response::new_err(id, code, message.to_owned());
    let _ = conn.sender.send(Message::Response(resp));
}
