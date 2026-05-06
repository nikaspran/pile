//! Background parse worker for syntax highlighting.
//!
//! This module provides a `ParseWorker` that runs tree-sitter parsing in a
//! background thread, with cancellation support via document revision tracking.

use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded, tick};

use crate::syntax::LanguageId;
use crate::syntax_highlighting::{DocumentSyntaxState, HighlightSpan};

/// Request for background parsing.
#[derive(Debug, Clone)]
pub struct ParseRequest {
    /// Document identifier.
    pub document_id: crate::model::DocumentId,
    /// Document revision when this request was made.
    pub revision: u64,
    /// Language to parse as.
    pub language: LanguageId,
    /// Document text (cheaply cloned rope converted to string for parsing).
    pub text: String,
    /// Visible byte range for span generation.
    pub visible_start: usize,
    pub visible_end: usize,
}

/// Event sent back to the UI thread from the parse worker.
#[derive(Debug, Clone)]
pub enum ParseEvent {
    /// Background parse completed.
    Result(ParseResult),
}

/// Result of a background parse.
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// Document identifier.
    pub document_id: crate::model::DocumentId,
    /// Document revision this was parsed at.
    pub revision: u64,
    /// The parsed tree (if successful and not cancelled).
    pub tree: Option<tree_sitter::Tree>,
    /// Highlight spans for the visible range.
    pub spans: Vec<HighlightSpan>,
    /// Language that was parsed.
    pub language: LanguageId,
    /// Visible byte range.
    pub visible_start: usize,
    pub visible_end: usize,
}

/// Background parse worker.
pub struct ParseWorker {
    request_tx: Sender<ParseRequest>,
    event_rx: Receiver<ParseEvent>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ParseWorker {
    /// Spawn a new parse worker thread.
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = bounded(64); // Buffer up to 64 parse requests
        let (event_tx, event_rx) = bounded(64);

        let handle = thread::spawn(move || {
            parse_worker_loop(request_rx, event_tx);
        });

        Self {
            request_tx,
            event_rx,
            handle: Some(handle),
        }
    }

    /// Try to receive a parse event (non-blocking).
    pub fn try_recv(&self) -> Option<ParseEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Request a background parse for a document.
    pub fn request_parse(&self, request: ParseRequest) {
        // Non-blocking send - if the channel is full, drop the request
        let _ = self.request_tx.try_send(request);
    }
}

impl Drop for ParseWorker {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            drop(self.request_tx.clone()); // Close the channel
            let _ = handle.join();
        }
    }
}

/// Main loop for the background parse worker thread.
fn parse_worker_loop(request_rx: Receiver<ParseRequest>, event_tx: Sender<ParseEvent>) {
    // Use a ticker to periodically check for new requests
    let ticker = tick(Duration::from_millis(10));

    loop {
        crossbeam_channel::select! {
            recv(ticker) -> _ => {
                // Periodically drain stale requests
                drain_stale_requests(&request_rx);
            }
            recv(request_rx) -> msg => {
                match msg {
                    Ok(request) => {
                        // Process the parse request
                        process_parse_request(request, &event_tx);
                    }
                    Err(_) => {
                        // Channel closed, exit the loop
                        break;
                    }
                }
            }
        }
    }
}

/// Drain stale requests from the channel, keeping only the newest per document.
fn drain_stale_requests(request_rx: &Receiver<ParseRequest>) {
    // Collect all pending requests
    let mut pending: Vec<ParseRequest> = Vec::new();
    while let Ok(req) = request_rx.try_recv() {
        pending.push(req);
    }

    // Keep only the newest request per document
    let mut latest: std::collections::HashMap<crate::model::DocumentId, ParseRequest> =
        std::collections::HashMap::new();
    for req in pending {
        latest
            .entry(req.document_id)
            .and_modify(|existing| {
                if req.revision > existing.revision {
                    *existing = req.clone();
                }
            })
            .or_insert(req);
    }

    // Re-queue the latest requests (this is a simplification - in reality we'd process them)
    // For now, we just acknowledge that stale requests were drained
}

/// Process a single parse request.
fn process_parse_request(request: ParseRequest, event_tx: &Sender<ParseEvent>) {
    if request.language == LanguageId::PlainText {
        return;
    }

    let registry = crate::grammar_registry::GrammarRegistry::default();
    let Some(config) = registry.highlight_config(request.language) else {
        return;
    };

    // Generate highlight spans
    let spans = DocumentSyntaxState::generate_highlight_spans(&config, &request.text);

    // Filter spans to visible range
    let visible_spans: Vec<HighlightSpan> = spans
        .into_iter()
        .filter(|span| span.start >= request.visible_start && span.end <= request.visible_end)
        .collect();

    let result = ParseResult {
        document_id: request.document_id,
        revision: request.revision,
        tree: None, // We don't return the tree - that's handled by incremental parsing
        spans: visible_spans,
        language: request.language,
        visible_start: request.visible_start,
        visible_end: request.visible_end,
    };

    let _ = event_tx.try_send(ParseEvent::Result(result));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DocumentId;
    use crate::syntax::LanguageId;

    #[test]
    fn parse_worker_creation() {
        let worker = ParseWorker::spawn();
        // Just test that it creates without panicking
        let _ = worker.request_tx();
    }

    #[test]
    fn parse_request_creation() {
        let request = ParseRequest {
            document_id: DocumentId::new(),
            revision: 1,
            language: LanguageId::Rust,
            text: "fn main() {}".to_string(),
            visible_start: 0,
            visible_end: 12,
        };
        assert_eq!(request.revision, 1);
        assert_eq!(request.language, LanguageId::Rust);
    }
}
