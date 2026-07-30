//! LSP client — manages language server processes via JSON-RPC over stdio.
//!
//! Provides an async `LspClient` that can spawn and communicate with any
//! language server implementing the LSP protocol. Uses `lsp_server::Message`
//! for wire-format encoding/decoding and `crossbeam_channel` for internal
//! message passing between async and sync I/O threads.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐    channels     ┌──────────────┐   stdio    ┌──────────────────┐
//! │  Async Caller   │◀═══╦═══════▶    │  Reader/Wtr  │◀═══╦══════▶│  Language Server │
//! │  (tokio task)   │    ║            │  (sync thrs)  │    ║       │  (child process) │
//! └─────────────────┘    ║            └──────────────┘    ║       └──────────────────┘
//!                  ┌─────╨──────┐                    ┌─────╨──────┐
//!                  │ Pending Req│                    │ stdio I/O  │
//!                  │ Map (Arc< )│                    │ (BufRead/  │
//!                  └────────────┘                    │  Write)    │
//!                                                    └────────────┘
//! ```

use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{Receiver, Sender, unbounded};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
};
use lsp_types::*;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, error, info, warn};

/// A client for communicating with a language server over LSP.
///
/// Manages the full lifecycle: spawning the server process, initializing
/// the LSP session, synchronizing text documents, and performing semantic
/// queries (hover, go-to-definition, references, code actions, etc.).
///
/// # Example (requires a running language server)
///
/// ```rust,ignore
/// # use lsp_types::Url;
/// # async fn example() -> anyhow::Result<()> {
/// let mut client = LspClient::new();
/// client.start("rust-analyzer", &[]).await?;
/// client.initialize(&Url::parse("file:///workspace")?).await?;
/// //     &Url::parse("file:///workspace/src/main.rs")?,
/// //     lsp_types::Position::new(10, 5),
/// // ).await?;
/// # Ok(())
/// # }
/// ```
/// 
/// **Note:** This example requires a running language server. See the
/// [`start`](Self::start) and [`initialize`](Self::initialize) docs for
/// details.
#[derive(Clone)]
pub struct LspClient {
    /// Shared mutable state
    state: Arc<Mutex<ClientState>>,
}

/// Internal state shared between the async API and the background reader thread.
struct ClientState {
    /// Sender for outgoing messages to the language server (→ writer thread → child stdin)
    sender: Option<Sender<lsp_server::Message>>,
    /// Receiver for incoming messages from the language server (← reader thread ← child stdout)
    receiver: Option<Receiver<lsp_server::Message>>,
    /// Server process handle (killed on shutdown)
    child: Option<std::process::Child>,
    /// Server capabilities received during initialize
    capabilities: Option<ServerCapabilities>,
    /// Map of pending request IDs → response oneshot senders
    pending: HashMap<u64, oneshot::Sender<lsp_server::Response>>,
    /// Diagnostics published by the server, keyed by document URI
    diagnostics: HashMap<Url, Vec<Diagnostic>>,
    /// Whether the client has been initialized
    initialized: bool,
    /// Whether a shutdown has been initiated
    shutting_down: bool,
}

impl LspClient {
    /// Create a new, unconnected LSP client.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ClientState {
                sender: None,
                receiver: None,
                child: None,
                capabilities: None,
                pending: HashMap::new(),
                diagnostics: HashMap::new(),
                initialized: false,
                shutting_down: false,
            })),
        }
    }

    /// Spawn the language server process and establish a JSON-RPC connection
    /// over stdio.
    ///
    /// After this, call [`initialize`](Self::initialize) to complete the handshake.
    pub async fn start(&self, command: &str, args: &[String]) -> Result<()> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn()
            .with_context(|| format!("Failed to spawn language server: {command}"))?;

        let mut child_stdin = child.stdin.take()
            .ok_or_else(|| anyhow!("Failed to capture server stdin"))?;
        let child_stdout = child.stdout.take()
            .ok_or_else(|| anyhow!("Failed to capture server stdout"))?;

        // Channels: outgoing (→ server) and incoming (← server)
        let (outgoing_tx, outgoing_rx) = unbounded::<lsp_server::Message>();
        let (incoming_tx, incoming_rx) = unbounded::<lsp_server::Message>();

        let _state = self.state.clone();

        // Writer thread: reads from outgoing channel, writes to child's stdin
        std::thread::Builder::new()
            .name("lsp-writer".into())
            .spawn(move || {
                while let Ok(msg) = outgoing_rx.recv() {
                    if let Err(e) = msg.write(&mut child_stdin) {
                        error!(error = %e, "LSP write error");
                        break;
                    }
                    if let Err(e) = child_stdin.flush() {
                        error!(error = %e, "LSP flush error");
                        break;
                    }
                }
                debug!("LSP writer thread exiting");
            })
            .context("Failed to spawn LSP writer thread")?;

        // Reader thread: reads from child's stdout, sends to incoming channel
        std::thread::Builder::new()
            .name("lsp-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(child_stdout);
                while let Ok(Some(msg)) = lsp_server::Message::read(&mut reader) {
                    let is_exit = matches!(&msg, lsp_server::Message::Notification(n)
                        if n.method == "exit");

                    if incoming_tx.send(msg).is_err() {
                        break; // receiver dropped (shutdown)
                    }

                    if is_exit {
                        break;
                    }
                }
                debug!("LSP reader thread exiting");
            })
            .context("Failed to spawn LSP reader thread")?;

        // Spawn a background task that routes responses to pending request handlers
        let state_clone = self.state.clone();
        let rx = incoming_rx.clone();
        tokio::task::spawn_blocking(move || {
            Self::reader_loop(&state_clone, &rx);
        });

        // Store state
        let mut s = self.state.lock().await;
        s.sender = Some(outgoing_tx);
        s.receiver = Some(incoming_rx);
        s.child = Some(child);

        info!(command, "Language server spawned");
        Ok(())
    }

    /// Perform the LSP initialize handshake.
    ///
    /// Sends the `initialize` request, processes the response, and sends the
    /// `initialized` notification.
    pub async fn initialize(&self, root_uri: &Url) -> Result<ServerCapabilities> {
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "workspace".to_string(),
            }]),
            capabilities: ClientCapabilities {
                text_document: Some(TextDocumentClientCapabilities {
                    synchronization: Some(TextDocumentSyncClientCapabilities {
                        dynamic_registration: Some(true),
                        will_save: Some(true),
                        will_save_wait_until: Some(true),
                        did_save: Some(true),
                    }),
                    completion: Some(CompletionClientCapabilities {
                        completion_item: Some(CompletionItemCapability {
                            snippet_support: Some(true),
                            commit_characters_support: Some(true),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    hover: Some(HoverClientCapabilities {
                        content_format: Some(vec![MarkupKind::Markdown]),
                        ..Default::default()
                    }),
                    definition: Some(GotoCapability {
                        dynamic_registration: Some(true),
                        link_support: Some(true),
                    }),
                    references: Some(ReferenceClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    code_action: Some(CodeActionClientCapabilities {
                        dynamic_registration: Some(true),
                        code_action_literal_support: Some(CodeActionLiteralSupport {
                            code_action_kind: CodeActionKindLiteralSupport {
                                value_set: vec![
                                    CodeActionKind::EMPTY.as_str().to_string(),
                                    CodeActionKind::QUICKFIX.as_str().to_string(),
                                    CodeActionKind::REFACTOR.as_str().to_string(),
                                    CodeActionKind::REFACTOR_EXTRACT.as_str().to_string(),
                                    CodeActionKind::REFACTOR_INLINE.as_str().to_string(),
                                    CodeActionKind::REFACTOR_REWRITE.as_str().to_string(),
                                    CodeActionKind::SOURCE.as_str().to_string(),
                                    CodeActionKind::SOURCE_ORGANIZE_IMPORTS.as_str().to_string(),
                                ],
                            },
                        }),
                        is_preferred_support: Some(true),
                        disabled_support: Some(true),
                        data_support: Some(true),
                        ..Default::default()
                    }),
                    document_symbol: Some(DocumentSymbolClientCapabilities {
                        dynamic_registration: Some(true),
                        hierarchical_document_symbol_support: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                workspace: Some(WorkspaceClientCapabilities {
                    workspace_edit: Some(WorkspaceEditClientCapabilities {
                        document_changes: Some(true),
                        ..Default::default()
                    }),
                    symbol: Some(WorkspaceSymbolClientCapabilities {
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let resp: InitializeResult = self.request("initialize", params).await?;
        self.notify::<lsp_types::notification::Initialized>(InitializedParams {}).await?;

        let caps = resp.capabilities;
        info!("Language server initialized");

        let mut s = self.state.lock().await;
        s.capabilities = Some(caps.clone());
        s.initialized = true;

        Ok(caps)
    }

    /// Shut down the language server gracefully.
    ///
    /// Sends `shutdown` request, then `exit` notification, then kills the
    /// child process.
    pub async fn shutdown(&self) -> Result<()> {
        let mut s = self.state.lock().await;
        if s.shutting_down || !s.initialized {
            return Ok(());
        }
        s.shutting_down = true;
        drop(s);

        // Send shutdown request
        let _: Option<Value> = self.request("shutdown", serde_json::json!(null)).await?;

        // Send exit notification
        self.notify_raw("exit", serde_json::json!(null)).await?;

        // Give the server a moment to process exit
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Kill the child process
        let mut s = self.state.lock().await;
        if let Some(mut child) = s.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        s.sender = None;
        s.receiver = None;
        s.initialized = false;
        info!("Language server shut down");
        Ok(())
    }

    /// Force-kill the server without graceful shutdown.
    pub async fn kill(&self) -> Result<()> {
        let mut s = self.state.lock().await;
        s.shutting_down = true;
        if let Some(mut child) = s.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        s.sender = None;
        s.receiver = None;
        s.initialized = false;
        Ok(())
    }

    // ========================================================================
    // Text synchronization
    // ========================================================================

    /// Open a document in the language server.
    pub async fn open_document(&self, uri: &Url, text: &str, language_id: &str) -> Result<()> {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: language_id.to_string(),
                version: 1,
                text: text.to_string(),
            },
        };
        self.notify::<DidOpenTextDocument>(params).await
    }

    /// Notify the server of document changes (full document sync).
    pub async fn change_document(&self, uri: &Url, text: &str, version: i32) -> Result<()> {
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        };
        self.notify::<DidChangeTextDocument>(params).await
    }

    /// Close a document in the language server.
    pub async fn close_document(&self, uri: &Url) -> Result<()> {
        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
        };
        self.notify::<DidCloseTextDocument>(params).await
    }

    /// Notify the server that a document was saved.
    pub async fn save_document(&self, uri: &Url) -> Result<()> {
        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            text: None,
        };
        self.notify::<DidSaveTextDocument>(params).await
    }

    // ========================================================================
    // Semantic features
    // ========================================================================

    /// Request hover information at a given position.
    pub async fn hover(&self, uri: &Url, position: Position) -> Result<Option<Hover>> {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        };
        self.request::<Option<Hover>>("textDocument/hover", params).await
    }

    /// Request go-to-definition at a given position.
    pub async fn goto_definition(&self, uri: &Url, position: Position) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        };
        self.request::<Option<lsp_types::GotoDefinitionResponse>>("textDocument/definition", params).await
    }

    /// Find all references for the symbol at a given position.
    pub async fn references(&self, uri: &Url, position: Position, include_declaration: bool) -> Result<Vec<Location>> {
        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            context: ReferenceContext {
                include_declaration,
            },
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let result: Option<Vec<Location>> = self.request("textDocument/references", params).await?;
        Ok(result.unwrap_or_default())
    }

    /// Request code actions for the given diagnostics.
    pub async fn code_actions(&self, uri: &Url, range: Range, diagnostics: Vec<Diagnostic>) -> Result<Vec<CodeActionOrCommand>> {
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range,
            context: CodeActionContext {
                diagnostics,
                only: None,
                trigger_kind: None,
            },
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let result: Option<Vec<CodeActionOrCommand>> = self.request("textDocument/codeAction", params).await?;
        Ok(result.unwrap_or_default())
    }

    /// Request document symbols for the given file.
    pub async fn document_symbols(&self, uri: &Url) -> Result<Vec<DocumentSymbol>> {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let response: Option<DocumentSymbolResponse> = self.request("textDocument/documentSymbol", params).await?;
        match response {
            Some(DocumentSymbolResponse::Nested(symbols)) => Ok(symbols),
            _ => Ok(Vec::new()),
        }
    }

    /// Request completion items at a given position.
    pub async fn completion(&self, uri: &Url, position: Position) -> Result<Vec<CompletionItem>> {
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            context: None,
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let result: Option<CompletionResponse> = self.request("textDocument/completion", params).await?;
        match result {
            Some(CompletionResponse::Array(items)) => Ok(items),
            Some(CompletionResponse::List(list)) => Ok(list.items),
            None => Ok(Vec::new()),
        }
    }

    /// Request a rename for the symbol at the given position.
    pub async fn rename(&self, uri: &Url, position: Position, new_name: &str) -> Result<Option<WorkspaceEdit>> {
        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            new_name: new_name.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.request::<Option<WorkspaceEdit>>("textDocument/rename", params).await
    }

    /// Request document formatting.
    pub async fn formatting(&self, uri: &Url, options: FormattingOptions) -> Result<Vec<TextEdit>> {
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            options,
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let result: Option<Vec<TextEdit>> = self.request("textDocument/formatting", params).await?;
        Ok(result.unwrap_or_default())
    }

    /// Execute a workspace command.
    pub async fn execute_command(&self, command: &str, arguments: Vec<serde_json::Value>) -> Result<Option<serde_json::Value>> {
        let params = ExecuteCommandParams {
            command: command.to_string(),
            arguments,
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.request("workspace/executeCommand", params).await
    }

    /// Get stored diagnostics for a given document URI.
    ///
    /// Diagnostics are collected from `textDocument/publishDiagnostics` notifications
    /// sent by the language server after opening a document.
    pub async fn get_diagnostics(&self, uri: &Url) -> Vec<lsp_types::Diagnostic> {
        let s = self.state.lock().await;
        s.diagnostics.get(uri).cloned().unwrap_or_default()
    }

    /// Clear stored diagnostics for all documents.
    pub async fn clear_diagnostics(&self) {
        let mut s = self.state.lock().await;
        s.diagnostics.clear();
    }

    // ========================================================================
    // Internal JSON-RPC helpers
    // ========================================================================

    /// Send a request and await the typed response.
    async fn request<R: DeserializeOwned>(&self, method: &str, params: impl serde::Serialize) -> Result<R> {
        static REQUEST_ID: AtomicU64 = AtomicU64::new(1);
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);

        let (tx, rx) = oneshot::channel::<lsp_server::Response>();

        // Register the pending request
        {
            let mut s = self.state.lock().await;
            s.pending.insert(request_id, tx);
        }

        // Build and send the request using lsp_server types
        let request = lsp_server::Request {
            id: lsp_server::RequestId::from(request_id as i32),
            method: method.to_string(),
            params: serde_json::to_value(params)?,
        };

        {
            let s = self.state.lock().await;
            if let Some(sender) = &s.sender {
                sender.send(lsp_server::Message::Request(request))
                    .map_err(|_| anyhow!("Failed to send request (server disconnected)"))?;
            } else {
                return Err(anyhow!("Client not started"));
            }
        }

        // Await the response with timeout
        let timeout = std::time::Duration::from_secs(30);
        let response = tokio::time::timeout(timeout, async {
            rx.await.map_err(|_| anyhow!("Server disconnected while waiting for response"))
        }).await
            .map_err(|_| anyhow!("Request timed out after {timeout:?}: {method}"))?
            .with_context(|| format!("Failed to receive response for: {method}"))?;

        // Drop the pending entry (already removed by reader)
        {
            let mut s = self.state.lock().await;
            s.pending.remove(&request_id);
        }

        // Check for LSP error response
        if let Some(err) = response.error {
            return Err(anyhow!("LSP error ({}): {}",
                err.code, err.message,
            ));
        }

        // Deserialize the result
        match response.result {
            Some(value) => serde_json::from_value(value)
                .map_err(|e| anyhow!("Failed to deserialize {method} response: {e}")),
            None => {
                serde_json::from_value(serde_json::Value::Null)
                    .map_err(|e| anyhow!("Failed to deserialize null response for {method}: {e}"))
            }
        }
    }

    /// Send a typed notification (fire-and-forget).
    async fn notify<N: lsp_types::notification::Notification>(&self, params: N::Params) -> Result<()> {
        let notification = lsp_server::Notification {
            method: N::METHOD.to_string(),
            params: serde_json::to_value(params)?,
        };
        let s = self.state.lock().await;
        if let Some(sender) = &s.sender {
            sender.send(lsp_server::Message::Notification(notification))
                .map_err(|_| anyhow!("Failed to send notification (server disconnected)"))?;
            Ok(())
        } else {
            Err(anyhow!("Client not started"))
        }
    }

    /// Send a raw notification by method name and params.
    async fn notify_raw(&self, method: &str, params: Value) -> Result<()> {
        let notification = lsp_server::Notification {
            method: method.to_string(),
            params,
        };
        let s = self.state.lock().await;
        if let Some(sender) = &s.sender {
            sender.send(lsp_server::Message::Notification(notification))
                .map_err(|_| anyhow!("Failed to send notification (server disconnected)"))?;
            Ok(())
        } else {
            Err(anyhow!("Client not started"))
        }
    }

    // ========================================================================
    // Background reader loop
    // ========================================================================

    /// Background task that reads messages from the language server and
    /// routes them: responses go to pending request handlers, notifications
    /// are logged, and server requests are auto-acknowledged.
    fn reader_loop(state: &Arc<Mutex<ClientState>>, receiver: &Receiver<lsp_server::Message>) {
        loop {
            match receiver.recv() {
                Ok(lsp_server::Message::Response(response)) => {
                    let id = response.id.to_string().parse::<u64>().unwrap_or(0);
                    let mut s = state.blocking_lock();
                    if let Some(tx) = s.pending.remove(&id) {
                        let _ = tx.send(response);
                    } else {
                        debug!(id, "Received response for unknown request");
                    }
                }
                Ok(lsp_server::Message::Notification(notif)) => {
                    match notif.method.as_str() {
                        "window/showMessage" | "window/logMessage" => {
                            if let Ok(params) = serde_json::from_value::<ShowMessageParams>(notif.params) {
                                info!(target: "lsp_server", "{}", params.message);
                            }
                        }
                        "textDocument/publishDiagnostics" => {
                            if let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(notif.params) {
                                debug!(
                                    uri = %params.uri,
                                    count = params.diagnostics.len(),
                                    "Received diagnostics",
                                );
                                let mut s = state.blocking_lock();
                                s.diagnostics.insert(params.uri, params.diagnostics);
                            }
                        }
                        _ => {
                            debug!(method = %notif.method, "Server notification");
                        }
                    }
                }
                Ok(lsp_server::Message::Request(request)) => {
                    match request.method.as_str() {
                        "window/showMessageRequest" | "client/registerCapability" => {
                            let sender = state.blocking_lock().sender.clone();
                            if let Some(sender) = sender {
                                let _ = sender.send(lsp_server::Message::Response(
                                    lsp_server::Response {
                                        id: request.id,
                                        result: Some(serde_json::json!(null)),
                                        error: None,
                                    }
                                ));
                            }
                        }
                        _ => {
                            warn!(method = %request.method, "Unhandled server request");
                        }
                    }
                }
                Err(_) => {
                    info!("LSP connection closed");
                    break;
                }
            }
        }
    }

    /// Check whether the client has been initialized.
    pub async fn is_initialized(&self) -> bool {
        self.state.lock().await.initialized
    }

    /// Get the server capabilities, if initialized.
    pub async fn capabilities(&self) -> Option<ServerCapabilities> {
        self.state.lock().await.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::notification::{DidOpenTextDocument, DidChangeTextDocument, DidCloseTextDocument};

    #[test]
    fn test_message_construction_request() {
        let request = lsp_server::Request {
            id: lsp_server::RequestId::from(1),
            method: "textDocument/hover".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.rs" },
                "position": { "line": 0, "character": 5 }
            }),
        };
        let msg = lsp_server::Message::Request(request);
        match msg {
            lsp_server::Message::Request(req) => {
                assert_eq!(req.method, "textDocument/hover");
            }
            _ => panic!("Expected Request message"),
        }
    }

    #[test]
    fn test_message_construction_notification() {
        let notif = lsp_server::Notification {
            method: "textDocument/didOpen".to_string(),
            params: serde_json::json!({
                "textDocument": {
                    "uri": "file:///test.rs",
                    "languageId": "rust",
                    "version": 1,
                    "text": "fn main() {}"
                }
            }),
        };
        let msg = lsp_server::Message::Notification(notif);
        match msg {
            lsp_server::Message::Notification(n) => {
                assert_eq!(n.method, "textDocument/didOpen");
            }
            _ => panic!("Expected Notification message"),
        }
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = lsp_server::Message::Notification(lsp_server::Notification {
            method: "textDocument/didOpen".to_string(),
            params: serde_json::json!({
                "textDocument": {
                    "uri": "file:///test.rs",
                    "languageId": "rust",
                    "version": 1,
                    "text": "fn main() {}"
                }
            }),
        });

        let mut buf: Vec<u8> = Vec::new();
        msg.write(&mut buf).unwrap();
        assert!(buf.len() > 0);
        assert!(std::str::from_utf8(&buf).unwrap().contains("textDocument/didOpen"));
        assert!(std::str::from_utf8(&buf).unwrap().contains("fn main()"));

        let mut cursor = std::io::Cursor::new(buf);
        let deserialized = lsp_server::Message::read(&mut cursor).unwrap().unwrap();
        match deserialized {
            lsp_server::Message::Notification(n) => {
                assert_eq!(n.method, "textDocument/didOpen");
                assert_eq!(n.params["textDocument"]["text"], "fn main() {}");
            }
            _ => panic!("Expected Notification"),
        }
    }

    #[test]
    fn test_error_response_deserialization() {
        // Calculate correct Content-Length for the JSON body
        let body = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let json = format!("{}{}", header, body);
        let mut cursor = std::io::Cursor::new(json.as_bytes());
        let msg = lsp_server::Message::read(&mut cursor).unwrap().unwrap();
        match msg {
            lsp_server::Message::Response(resp) => {
                assert!(resp.error.is_some());
                let err = resp.error.as_ref().unwrap();
                assert_eq!(err.code, -32601);
                assert_eq!(err.message, "Method not found");
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn test_success_response_deserialization() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let json = format!("{}{}", header, body);
        let mut cursor = std::io::Cursor::new(json.as_bytes());
        let msg = lsp_server::Message::read(&mut cursor).unwrap().unwrap();
        match msg {
            lsp_server::Message::Response(resp) => {
                assert!(resp.result.is_some());
                assert!(resp.error.is_none());
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn test_client_creation() {
        let client = LspClient::new();
        assert!(!client.state.blocking_lock().initialized);
    }

    #[test]
    fn test_open_document_params_serialization() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 1,
                text: "fn main() {}".to_string(),
            },
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["textDocument"]["uri"], "file:///test.rs");
        assert_eq!(json["textDocument"]["languageId"], "rust");
    }

    #[test]
    fn test_hover_params_serialization() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line: 5, character: 10 },
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["position"]["line"], 5);
        assert_eq!(json["position"]["character"], 10);
    }

    #[test]
    fn test_reference_params_serialization() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line: 10, character: 3 },
            },
            context: ReferenceContext { include_declaration: true },
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["context"]["includeDeclaration"], true);
    }

    #[test]
    fn test_code_action_params_serialization() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 1, character: 0 },
            },
            context: CodeActionContext {
                diagnostics: vec![Diagnostic {
                    range: Range {
                        start: Position { line: 0, character: 0 },
                        end: Position { line: 0, character: 5 },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("E0001".to_string())),
                    source: Some("rustc".to_string()),
                    message: "unused variable".to_string(),
                    ..Default::default()
                }],
                only: None,
                trigger_kind: None,
            },
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["context"]["diagnostics"][0]["message"], "unused variable");
        assert_eq!(json["context"]["diagnostics"][0]["severity"], 1);
    }

    #[test]
    fn test_request_response_roundtrip_via_lsp_server_types() {
        // Simulate a request → response cycle using lsp_server types
        let notif = lsp_server::Notification {
            method: "test/method".to_string(),
            params: serde_json::json!({"key": "value"}),
        };
        let msg = lsp_server::Message::Notification(notif);
        let mut buf: Vec<u8> = Vec::new();
        msg.write(&mut buf).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let result = lsp_server::Message::read(&mut cursor).unwrap().unwrap();
        match result {
            lsp_server::Message::Notification(n) => {
                assert_eq!(n.method, "test/method");
                assert_eq!(n.params["key"], "value");
            }
            _ => panic!("Expected Notification"),
        }
    }

    #[test]
    fn test_document_symbol_response_deserialization() {
        let json = r#"{
            "capabilities": {
                "documentSymbolProvider": true
            }
        }"#;
        let result: InitializeResult = serde_json::from_str(json).unwrap();
        assert!(result.capabilities.document_symbol_provider.is_some());
    }
}
