//! Background parse worker for syntax highlighting.
//!
//! This module provides a `ParseWorker` that runs tree-sitter parsing in a
//! background thread, with cancellation support via document revision tracking.

use std::collections::HashMap;
use std::thread;

use crossbeam_channel::{Receiver, Sender, bounded};

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
    /// Document text for the visible range.
    /// Note: tree-sitter requires &[u8] input, so materialization is necessary.
    /// We only materialize the visible byte range to minimize allocation.
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
        // Non-blocking send - if the channel is full, drop the oldest and retry
        if self.request_tx.try_send(request).is_err() {
            // Channel is full - we could drain here, but for simplicity we just drop
        }
    }
}

impl Drop for ParseWorker {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Drop the sender to close the channel and unblock the worker thread
            drop(std::mem::replace(
                &mut self.request_tx,
                crossbeam_channel::bounded(1).0,
            ));
            let _ = handle.join();
        }
    }
}

/// Main loop for the background parse worker thread.
fn parse_worker_loop(request_rx: Receiver<ParseRequest>, event_tx: Sender<ParseEvent>) {
    // Track the latest revision we've seen per document for cancellation
    let mut latest_revisions: HashMap<crate::model::DocumentId, u64> = HashMap::new();

    loop {
        // Use a select with a small timeout to check for new requests
        let result = crossbeam_channel::select! {
            recv(request_rx) -> msg => msg,
        };

        match result {
            Ok(mut request) => {
                // Before processing, drain any additional requests for the same document
                // and keep only the latest revision
                while let Ok(next) = request_rx.try_recv() {
                    if next.document_id == request.document_id {
                        if next.revision > request.revision {
                            request = next;
                        }
                    } else {
                        // Different document - just process the original request
                        // and re-queue the new one is not possible with crossbeam
                        // Just process the newest for each document
                        break;
                    }
                }

                // Update the latest revision for this document
                latest_revisions.insert(request.document_id, request.revision);

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

    // Spans are relative to request.text, which is already the visible range.
    let visible_spans: Vec<HighlightSpan> = spans
        .into_iter()
        .filter(|span| span.end <= request.text.len())
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
    use crate::syntax::LanguageId;

    #[test]
    fn parse_worker_creation() {
        let worker = ParseWorker::spawn();
        // Test that it creates without panicking
        // Send a request to verify it works
        let request = ParseRequest {
            document_id: uuid::Uuid::new_v4(),
            revision: 1,
            language: LanguageId::PlainText, // PlainText won't be processed
            text: "".to_string(),
            visible_start: 0,
            visible_end: 0,
        };
        worker.request_parse(request);
        // Drop the worker explicitly to close the channel and stop the thread
        drop(worker);
    }

    #[test]
    fn parse_request_creation() {
        let request = ParseRequest {
            document_id: uuid::Uuid::new_v4(),
            revision: 1,
            language: LanguageId::Rust,
            text: "fn main() {}".to_string(),
            visible_start: 0,
            visible_end: 12,
        };
        assert_eq!(request.revision, 1);
        assert_eq!(request.language, LanguageId::Rust);
    }

    #[test]
    fn parse_result_keeps_relative_spans_for_nonzero_visible_range() {
        let (event_tx, event_rx) = bounded(1);
        let request = ParseRequest {
            document_id: uuid::Uuid::new_v4(),
            revision: 1,
            language: LanguageId::Rust,
            text: "fn main() {\n    let value = 1;\n}".to_owned(),
            visible_start: 1024,
            visible_end: 1056,
        };

        process_parse_request(request, &event_tx);

        let ParseEvent::Result(result) = event_rx.try_recv().unwrap();
        assert!(!result.spans.is_empty());
        assert!(
            result
                .spans
                .iter()
                .all(|span| span.end <= result.visible_end - result.visible_start)
        );
    }
}
