mod capabilities;
mod convert;
mod documents;
mod handlers;

use crate::error::{MarkdownlintError, Result};
use documents::DocumentStore;
use lsp_server::{Connection, IoThreads, Message};

/// Start an LSP server on stdio.
pub fn run_server() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();
    run_server_with_connection(connection, Some(io_threads))
}

/// Run the LSP event loop on an existing connection.
///
/// Exposed for integration testing via `Connection::memory()`.
pub fn run_server_with_connection(
    connection: Connection,
    io_threads: Option<IoThreads>,
) -> Result<()> {
    let server_capabilities = serde_json::to_value(capabilities::capabilities())
        .map_err(|e| MarkdownlintError::Lsp(e.to_string()))?;

    connection
        .initialize(server_capabilities)
        .map_err(|e| MarkdownlintError::Lsp(e.to_string()))?;

    let mut docs = DocumentStore::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection
                    .handle_shutdown(&req)
                    .map_err(|e| MarkdownlintError::Lsp(e.to_string()))?
                {
                    if let Some(threads) = io_threads {
                        threads
                            .join()
                            .map_err(|e| MarkdownlintError::Lsp(format!("{e}")))?;
                    }
                    return Ok(());
                }
                handlers::handle_request(&connection, &req, &mut docs);
            }
            Message::Notification(notif) => {
                if notif.method == "exit" {
                    if let Some(threads) = io_threads {
                        threads
                            .join()
                            .map_err(|e| MarkdownlintError::Lsp(format!("{e}")))?;
                    }
                    return Ok(());
                }
                handlers::handle_notification(&connection, &notif, &mut docs);
            }
            Message::Response(_) => {}
        }
    }

    Ok(())
}
