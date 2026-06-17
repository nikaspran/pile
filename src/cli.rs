use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand, value_parser};
use crop::Rope;
use serde::Serialize;

use crate::{
    model::{AppState, Document, DocumentId},
    persistence::{decode_session_snapshot, default_session_path},
    search::{SearchOptions, find_matches},
};

const DEFAULT_SEARCH_LIMIT: usize = 50;
const DEFAULT_CONTEXT_CHARS: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputFormat {
    Json,
    Human,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum DocumentStatus {
    Open,
    Closed,
}

#[derive(Clone, Copy)]
struct DocumentView<'a> {
    document: &'a Document,
    status: DocumentStatus,
    tab_index: Option<usize>,
}

#[derive(Serialize)]
struct ListOutput {
    documents: Vec<DocumentSummary>,
}

#[derive(Serialize)]
struct DocumentSummary {
    document_id: DocumentId,
    title: String,
    status: DocumentStatus,
    tab_index: Option<usize>,
    line_count: usize,
    byte_len: usize,
}

#[derive(Serialize)]
struct SearchOutput {
    query: String,
    total_matches: usize,
    returned_matches: usize,
    matches: Vec<SearchOutputMatch>,
}

#[derive(Serialize)]
struct SearchOutputMatch {
    document_id: DocumentId,
    title: String,
    status: DocumentStatus,
    tab_index: Option<usize>,
    line_number: usize,
    match_start: usize,
    match_end: usize,
    matched_text: String,
    context_before: String,
    context_after: String,
}

#[derive(Serialize)]
struct GetOutput {
    document_id: DocumentId,
    title: String,
    status: DocumentStatus,
    tab_index: Option<usize>,
    line_count: usize,
    byte_len: usize,
    lines: Option<LineRangeOutput>,
    content: String,
}

#[derive(Serialize)]
struct LineRangeOutput {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineRange {
    start: usize,
    end: usize,
}

pub fn run_from_env() -> Result<()> {
    run_from(std::env::args_os(), &mut std::io::stdout())
}

pub fn run_from<I, T, W>(args: I, out: &mut W) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
    W: Write,
{
    let matches = match command().try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            write!(out, "{err}")?;
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };

    match matches.subcommand() {
        Some(("list", sub)) => run_list(sub, out),
        Some(("search", sub)) => run_search(sub, out),
        Some(("get", sub)) => run_get(sub, out),
        _ => Ok(()),
    }
}

fn command() -> ClapCommand {
    ClapCommand::new("pile")
        .about("A minimalist infinite scratchpad editor.")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            ClapCommand::new("list")
                .about("List scratch buffers from the persisted session")
                .arg(closed_arg())
                .arg(format_arg())
                .arg(session_arg()),
        )
        .subcommand(
            ClapCommand::new("search")
                .about("Search scratch buffers from the persisted session")
                .arg(Arg::new("query").required(true))
                .arg(closed_arg())
                .arg(format_arg())
                .arg(session_arg())
                .arg(
                    Arg::new("case-sensitive")
                        .long("case-sensitive")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("whole-word")
                        .long("whole-word")
                        .action(ArgAction::SetTrue),
                )
                .arg(Arg::new("regex").long("regex").action(ArgAction::SetTrue))
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_parser(value_parser!(usize))
                        .default_value("50"),
                )
                .arg(
                    Arg::new("context")
                        .long("context")
                        .value_parser(value_parser!(usize))
                        .default_value("80"),
                ),
        )
        .subcommand(
            ClapCommand::new("get")
                .about("Retrieve scratch buffer content from the persisted session")
                .arg(Arg::new("document-id").required(true))
                .arg(closed_arg())
                .arg(format_arg())
                .arg(session_arg())
                .arg(
                    Arg::new("lines")
                        .long("lines")
                        .value_name("START:END")
                        .help("Return a 1-based inclusive line range"),
                ),
        )
}

fn closed_arg() -> Arg {
    Arg::new("closed")
        .long("closed")
        .help("Include recently closed scratch buffers")
        .action(ArgAction::SetTrue)
}

fn format_arg() -> Arg {
    Arg::new("format")
        .long("format")
        .value_parser(["json", "human"])
        .default_value("json")
}

fn session_arg() -> Arg {
    Arg::new("session")
        .long("session")
        .value_name("PATH")
        .value_parser(value_parser!(PathBuf))
        .help("Read a specific pile session file")
}

fn run_list<W: Write>(matches: &ArgMatches, out: &mut W) -> Result<()> {
    let state = load_state(session_path(matches)?)?;
    let documents = state
        .as_ref()
        .map(|state| document_views(state, include_closed(matches)))
        .unwrap_or_default();
    let summaries = documents.iter().map(|view| summarize(*view)).collect();
    let output = ListOutput {
        documents: summaries,
    };

    match output_format(matches)? {
        OutputFormat::Json => write_json(out, &output),
        OutputFormat::Human => {
            for document in output.documents {
                writeln!(
                    out,
                    "{}\t{}\t{}\t{} lines\t{} bytes",
                    document.document_id,
                    status_label(document.status),
                    document.title,
                    document.line_count,
                    document.byte_len
                )?;
            }
            Ok(())
        }
    }
}

fn run_search<W: Write>(matches: &ArgMatches, out: &mut W) -> Result<()> {
    let state = load_state(session_path(matches)?)?;
    let query = matches
        .get_one::<String>("query")
        .context("missing search query")?;
    let options = SearchOptions {
        case_sensitive: matches.get_flag("case-sensitive"),
        whole_word: matches.get_flag("whole-word"),
        use_regex: matches.get_flag("regex"),
    };
    if options.use_regex {
        regex::Regex::new(query).context("invalid regex query")?;
    }
    let limit = *matches
        .get_one::<usize>("limit")
        .unwrap_or(&DEFAULT_SEARCH_LIMIT);
    let context_chars = *matches
        .get_one::<usize>("context")
        .unwrap_or(&DEFAULT_CONTEXT_CHARS);

    let mut total_matches = 0;
    let mut output_matches = Vec::new();
    let documents = state
        .as_ref()
        .map(|state| document_views(state, include_closed(matches)))
        .unwrap_or_default();
    for view in documents {
        let document_matches = find_matches(&view.document.rope, query, options);
        total_matches += document_matches.len();

        for search_match in document_matches {
            if output_matches.len() >= limit {
                continue;
            }
            output_matches.push(search_output_match(view, search_match, context_chars));
        }
    }

    let output = SearchOutput {
        query: query.clone(),
        total_matches,
        returned_matches: output_matches.len(),
        matches: output_matches,
    };

    match output_format(matches)? {
        OutputFormat::Json => write_json(out, &output),
        OutputFormat::Human => {
            for result in output.matches {
                writeln!(
                    out,
                    "{}:{}:{} [{}] {}{}{}",
                    result.document_id,
                    result.line_number,
                    result.match_start,
                    status_label(result.status),
                    result.context_before,
                    result.matched_text,
                    result.context_after
                )?;
            }
            Ok(())
        }
    }
}

fn run_get<W: Write>(matches: &ArgMatches, out: &mut W) -> Result<()> {
    let state = load_state(session_path(matches)?)?;
    let document_id = matches
        .get_one::<String>("document-id")
        .context("missing document id")?
        .parse::<DocumentId>()
        .context("invalid document id")?;
    let line_range = matches
        .get_one::<String>("lines")
        .map(|value| parse_line_range(value))
        .transpose()?;
    let documents = state
        .as_ref()
        .map(|state| document_views(state, include_closed(matches)))
        .unwrap_or_default();
    let view = documents
        .into_iter()
        .find(|view| view.document.id == document_id)
        .with_context(|| format!("document not found: {document_id}"))?;
    let line_count = line_count(&view.document.rope);
    let content = match line_range {
        Some(range) => content_for_line_range(&view.document.rope, range)?,
        None => view.document.rope.to_string(),
    };
    let output = GetOutput {
        document_id: view.document.id,
        title: view.document.display_title(),
        status: view.status,
        tab_index: view.tab_index,
        line_count,
        byte_len: view.document.rope.byte_len(),
        lines: line_range.map(|range| LineRangeOutput {
            start: range.start,
            end: range.end.min(line_count),
        }),
        content,
    };

    match output_format(matches)? {
        OutputFormat::Json => write_json(out, &output),
        OutputFormat::Human => {
            write!(out, "{}", output.content)?;
            Ok(())
        }
    }
}

fn load_state(path: PathBuf) -> Result<Option<AppState>> {
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };
    let mut snapshot = decode_session_snapshot(&bytes)?;
    snapshot.state.validate();
    Ok(Some(snapshot.state))
}

fn session_path(matches: &ArgMatches) -> Result<PathBuf> {
    Ok(matches
        .get_one::<PathBuf>("session")
        .cloned()
        .unwrap_or_else(default_session_path))
}

fn include_closed(matches: &ArgMatches) -> bool {
    matches.get_flag("closed")
}

fn output_format(matches: &ArgMatches) -> Result<OutputFormat> {
    match matches
        .get_one::<String>("format")
        .map(|value| value.as_str())
        .unwrap_or("json")
    {
        "json" => Ok(OutputFormat::Json),
        "human" => Ok(OutputFormat::Human),
        other => anyhow::bail!("unsupported output format: {other}"),
    }
}

fn document_views(state: &AppState, include_closed: bool) -> Vec<DocumentView<'_>> {
    let mut documents = Vec::new();
    for (tab_index, document_id) in state.tab_order.iter().enumerate() {
        if let Some(document) = state.document(*document_id) {
            documents.push(DocumentView {
                document,
                status: DocumentStatus::Open,
                tab_index: Some(tab_index),
            });
        }
    }

    if include_closed {
        let mut closed_documents: Vec<_> = state.closed_documents().iter().collect();
        closed_documents.sort_by_key(|closed| std::cmp::Reverse(closed.order));
        for closed in closed_documents {
            documents.push(DocumentView {
                document: &closed.document,
                status: DocumentStatus::Closed,
                tab_index: None,
            });
        }
    }

    documents
}

fn summarize(view: DocumentView<'_>) -> DocumentSummary {
    DocumentSummary {
        document_id: view.document.id,
        title: view.document.display_title(),
        status: view.status,
        tab_index: view.tab_index,
        line_count: line_count(&view.document.rope),
        byte_len: view.document.rope.byte_len(),
    }
}

fn search_output_match(
    view: DocumentView<'_>,
    search_match: crate::search::SearchMatch,
    context_chars: usize,
) -> SearchOutputMatch {
    let rope = &view.document.rope;
    let rope_len = rope.byte_len();
    let context_start = floor_char_boundary(rope, search_match.start.saturating_sub(context_chars));
    let context_end = floor_char_boundary(rope, (search_match.end + context_chars).min(rope_len));

    SearchOutputMatch {
        document_id: view.document.id,
        title: view.document.display_title(),
        status: view.status,
        tab_index: view.tab_index,
        line_number: line_number_for_byte(rope, search_match.start),
        match_start: search_match.start,
        match_end: search_match.end,
        matched_text: rope
            .byte_slice(search_match.start..search_match.end)
            .to_string(),
        context_before: rope
            .byte_slice(context_start..search_match.start)
            .to_string(),
        context_after: rope.byte_slice(search_match.end..context_end).to_string(),
    }
}

fn line_count(rope: &Rope) -> usize {
    rope.line_len().max(1)
}

fn line_number_for_byte(rope: &Rope, byte_offset: usize) -> usize {
    if rope.byte_len() == 0 {
        return 1;
    }
    let byte_offset = floor_char_boundary(rope, byte_offset.min(rope.byte_len()));
    rope.line_of_byte(byte_offset) + 1
}

fn parse_line_range(value: &str) -> Result<LineRange> {
    let (start, end) = value
        .split_once(':')
        .with_context(|| format!("invalid line range {value:?}; expected START:END"))?;
    let start = start
        .parse::<usize>()
        .with_context(|| format!("invalid line range start: {start:?}"))?;
    let end = end
        .parse::<usize>()
        .with_context(|| format!("invalid line range end: {end:?}"))?;
    if start == 0 || end == 0 || end < start {
        anyhow::bail!("invalid line range {value:?}; expected 1-based START:END");
    }
    Ok(LineRange { start, end })
}

fn content_for_line_range(rope: &Rope, range: LineRange) -> Result<String> {
    let total_lines = line_count(rope);
    if range.start > total_lines {
        anyhow::bail!(
            "line range starts past end of document: {} > {}",
            range.start,
            total_lines
        );
    }

    let start_line = range.start - 1;
    let end_line_exclusive = range.end.min(total_lines);
    let start_byte = rope.byte_of_line(start_line);
    let end_byte = if end_line_exclusive >= rope.line_len() {
        rope.byte_len()
    } else {
        rope.byte_of_line(end_line_exclusive)
    };
    Ok(rope.byte_slice(start_byte..end_byte).to_string())
}

fn floor_char_boundary(rope: &Rope, mut offset: usize) -> usize {
    offset = offset.min(rope.byte_len());
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn write_json<W: Write, T: Serialize>(out: &mut W, value: &T) -> Result<()> {
    serde_json::to_writer_pretty(&mut *out, value)?;
    writeln!(out)?;
    Ok(())
}

fn status_label(status: DocumentStatus) -> &'static str {
    match status {
        DocumentStatus::Open => "open",
        DocumentStatus::Closed => "closed",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        model::{AppState, ClosedDocument, Document, SessionSnapshot},
        persistence::SessionEnvelope,
    };

    fn write_session(state: AppState) -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".session.bin");
        let snapshot = SessionSnapshot {
            schema_version: 4,
            state,
            panes: vec![],
            active_pane: 0,
        };
        let bytes = SessionEnvelope::wrap(&snapshot)
            .unwrap()
            .to_bytes()
            .unwrap();
        fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    fn document(title: &str, text: &str) -> Document {
        let mut document = Document::new_untitled(1, 4, true);
        document.rename(title);
        document.replace_text(text);
        document
    }

    fn run(args: Vec<String>) -> Value {
        let mut out = Vec::new();
        run_from(args, &mut out).unwrap();
        serde_json::from_slice(&out).unwrap()
    }

    fn run_text(args: Vec<String>) -> String {
        let mut out = Vec::new();
        run_from(args, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn top_level_help_is_successful_output() {
        let output = run_text(vec!["pile".into(), "--help".into()]);

        assert!(output.contains("Usage: pile <COMMAND>"));
        assert!(output.contains("Commands:"));
    }

    #[test]
    fn subcommand_help_is_successful_output() {
        let output = run_text(vec!["pile".into(), "search".into(), "--help".into()]);

        assert!(output.contains("Usage: pile search [OPTIONS] <query>"));
        assert!(output.contains("--case-sensitive"));
    }

    #[test]
    fn version_is_successful_output() {
        let output = run_text(vec!["pile".into(), "--version".into()]);

        assert!(output.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn list_defaults_to_open_documents_only() {
        let open = document("Open", "alpha\nbeta");
        let closed = document("Closed", "hidden");
        let open_id = open.id;
        let closed_id = closed.id;
        let state = AppState {
            documents: vec![open],
            tab_order: vec![open_id],
            active_document: open_id,
            next_untitled_index: 2,
            recent_order: vec![open_id],
            closed_documents: vec![ClosedDocument {
                document: closed,
                order: 0,
            }],
            next_closed_order: 1,
        };
        let (_dir, path) = write_session(state);

        let json = run(vec![
            "pile".into(),
            "list".into(),
            "--session".into(),
            path.display().to_string(),
        ]);

        assert_eq!(json["documents"].as_array().unwrap().len(), 1);
        assert_eq!(json["documents"][0]["document_id"], open_id.to_string());
        assert_ne!(json["documents"][0]["document_id"], closed_id.to_string());
    }

    #[test]
    fn list_includes_closed_when_requested() {
        let open = document("Open", "alpha");
        let closed = document("Closed", "hidden");
        let open_id = open.id;
        let state = AppState {
            documents: vec![open],
            tab_order: vec![open_id],
            active_document: open_id,
            next_untitled_index: 2,
            recent_order: vec![open_id],
            closed_documents: vec![ClosedDocument {
                document: closed,
                order: 0,
            }],
            next_closed_order: 1,
        };
        let (_dir, path) = write_session(state);

        let json = run(vec![
            "pile".into(),
            "list".into(),
            "--closed".into(),
            "--session".into(),
            path.display().to_string(),
        ]);

        assert_eq!(json["documents"].as_array().unwrap().len(), 2);
        assert_eq!(json["documents"][1]["status"], "closed");
    }

    #[test]
    fn search_returns_matches_with_context_and_limit() {
        let doc = document("Notes", "alpha beta\nbeta gamma\nbeta delta");
        let doc_id = doc.id;
        let state = AppState {
            documents: vec![doc],
            tab_order: vec![doc_id],
            active_document: doc_id,
            next_untitled_index: 2,
            recent_order: vec![doc_id],
            closed_documents: vec![],
            next_closed_order: 0,
        };
        let (_dir, path) = write_session(state);

        let json = run(vec![
            "pile".into(),
            "search".into(),
            "beta".into(),
            "--limit".into(),
            "2".into(),
            "--context".into(),
            "3".into(),
            "--session".into(),
            path.display().to_string(),
        ]);

        assert_eq!(json["total_matches"], 3);
        assert_eq!(json["returned_matches"], 2);
        assert_eq!(json["matches"][0]["line_number"], 1);
        assert_eq!(json["matches"][1]["line_number"], 2);
    }

    #[test]
    fn get_returns_line_range() {
        let doc = document("Notes", "one\ntwo\nthree\n");
        let doc_id = doc.id;
        let state = AppState {
            documents: vec![doc],
            tab_order: vec![doc_id],
            active_document: doc_id,
            next_untitled_index: 2,
            recent_order: vec![doc_id],
            closed_documents: vec![],
            next_closed_order: 0,
        };
        let (_dir, path) = write_session(state);

        let json = run(vec![
            "pile".into(),
            "get".into(),
            doc_id.to_string(),
            "--lines".into(),
            "2:3".into(),
            "--session".into(),
            path.display().to_string(),
        ]);

        assert_eq!(json["content"], "two\nthree\n");
        assert_eq!(json["lines"]["start"], 2);
        assert_eq!(json["lines"]["end"], 3);
    }

    #[test]
    fn missing_session_lists_no_documents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-session.bin");

        let json = run(vec![
            "pile".into(),
            "list".into(),
            "--session".into(),
            path.display().to_string(),
        ]);

        assert_eq!(json["documents"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn invalid_regex_is_reported() {
        let doc = document("Notes", "alpha");
        let doc_id = doc.id;
        let state = AppState {
            documents: vec![doc],
            tab_order: vec![doc_id],
            active_document: doc_id,
            next_untitled_index: 2,
            recent_order: vec![doc_id],
            closed_documents: vec![],
            next_closed_order: 0,
        };
        let (_dir, path) = write_session(state);
        let mut out = Vec::new();

        let err = run_from(
            vec![
                "pile".to_owned(),
                "search".to_owned(),
                "[".to_owned(),
                "--regex".to_owned(),
                "--session".to_owned(),
                path.display().to_string(),
            ],
            &mut out,
        )
        .unwrap_err();

        assert!(err.to_string().contains("invalid regex query"));
    }
}
