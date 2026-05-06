use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

mod common;
use common::*;

fn bench_editing_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("editing_latency");

    // Benchmark single character insertion at end of buffer
    for &size in [1_000, 10_000, 100_000].iter() {
        group.bench_with_input(
            BenchmarkId::new("insert_char_at_end", format_size(size)),
            &size,
            |b, &size| {
                let mut rope = crop::Rope::from(generate_text(size));
                let text = "a";
                b.iter(|| {
                    rope.insert(rope.byte_len(), black_box(text));
                    black_box(&rope);
                })
            },
        );
    }

    // Benchmark rope line iteration (for rendering)
    for &size in [1_000, 10_000, 100_000].iter() {
        group.bench_with_input(
            BenchmarkId::new("iterate_lines", format_size(size)),
            &size,
            |b, &size| {
                let rope = crop::Rope::from(generate_text(size));
                b.iter(|| {
                    for line in rope.lines() {
                        black_box(line);
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_startup_restore(c: &mut Criterion) {
    let mut group = c.benchmark_group("startup_restore");

    // Benchmark session deserialization with different tab counts
    for &tab_count in [1, 10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("deserialize_session", format!("{} tabs", tab_count)),
            &tab_count,
            |b, &tab_count| {
                let snapshot = create_test_snapshot(tab_count, 1_000);
                let serialized = bincode::serialize(&snapshot).unwrap();
                b.iter(|| {
                    let result: Result<pile::model::SessionSnapshot, bincode::Error> =
                        bincode::deserialize(black_box(&serialized));
                    black_box(result)
                })
            },
        );
    }

    group.finish();
}

fn bench_syntax_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("syntax_parse");

    // Benchmark tree-sitter parsing for Rust code
    let test_cases = [("rust_1k", 1_000), ("rust_10k", 10_000)];

    for (name, lines) in test_cases.iter() {
        let code = generate_rust_code(*lines);
        group.bench_with_input(
            BenchmarkId::new("parse_rust", name),
            &code,
            |b, code| {
                b.iter(|| {
                    black_box(parse_syntax(black_box(code)))
                })
            },
        );
    }

    group.finish();
}

fn bench_search_time(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_time");

    // Benchmark in-buffer search with different buffer sizes
    for &size in [1_000, 10_000, 100_000].iter() {
        let text = generate_text(size);
        group.bench_with_input(
            BenchmarkId::new("in_buffer_search", format_size(size)),
            &text,
            |b, text| {
                b.iter(|| {
                    black_box(search_text(black_box(text), black_box("the")))
                })
            },
        );
    }

    group.finish();
}

fn bench_memory_use(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_use");

    // Estimate memory for different numbers of tabs
    for &tab_count in [10, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::new("session_memory", format!("{} tabs", tab_count)),
            &tab_count,
            |b, &tab_count| {
                b.iter(|| {
                    let snapshot = create_test_snapshot(tab_count, 1_000);
                    let serialized = bincode::serialize(&snapshot).unwrap();
                    black_box(serialized.len())
                })
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2));
    targets = bench_editing_latency, bench_startup_restore, bench_syntax_parse, bench_search_time, bench_memory_use
}

criterion_main!(benches);

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{}MB", bytes / (1024 * 1024))
    }
}
