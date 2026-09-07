// Spec: specs/008-change-detection/spec.md

//! Spec 008 AC-2: the cost of one `evaluate` at the configured ceiling.
//!
//! §3.2 bounds the detector by `max_compare_chars` squared and claims
//! "single-digit milliseconds" at the 6000-character default. This benchmark
//! is where that claim is measured rather than asserted. It is deliberately
//! not gated in CI: it records a number for `docs/architecture.md`, and a
//! micro-benchmark on a shared runner is not a pass/fail signal.
//!
//! Run with `cargo bench -p butler-core`.

use butler_core::delta::{
    ChangeDetector, DEFAULT_MAX_COMPARE_CHARS, DEFAULT_STABILITY_FRAMES, DEFAULT_THRESHOLD,
    DetectorInput, LevenshteinDetector,
};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// A 6000-character screen of prose: the worst case the default allows.
fn screen(seed: char) -> String {
    let line = format!("{seed} the quick brown fox jumps over the lazy dog while it rains");
    let mut s = String::with_capacity(DEFAULT_MAX_COMPARE_CHARS + 128);
    while s.chars().count() < DEFAULT_MAX_COMPARE_CHARS {
        s.push_str(&line);
        s.push('\n');
    }
    s.chars().take(DEFAULT_MAX_COMPARE_CHARS).collect()
}

fn bench_evaluate(c: &mut Criterion) {
    let base = screen('a');
    let other = screen('z');

    let mut group = c.benchmark_group("delta");

    // The worst case: two full-length, maximally dissimilar screens, which is
    // the full max_compare_chars squared matrix.
    group.bench_function("evaluate_6000_vs_6000_dissimilar", |b| {
        b.iter_batched_ref(
            || {
                let mut d = LevenshteinDetector::new(
                    DEFAULT_THRESHOLD,
                    DEFAULT_STABILITY_FRAMES,
                    DEFAULT_MAX_COMPARE_CHARS,
                );
                d.commit(&base);
                d
            },
            |d| {
                black_box(d.evaluate(black_box(&other), &DetectorInput::default()));
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // The common case: the screen has not changed, which is what the detector
    // spends almost all of its calls deciding.
    group.bench_function("evaluate_6000_identical", |b| {
        b.iter_batched_ref(
            || {
                let mut d = LevenshteinDetector::new(
                    DEFAULT_THRESHOLD,
                    DEFAULT_STABILITY_FRAMES,
                    DEFAULT_MAX_COMPARE_CHARS,
                );
                d.commit(&base);
                d
            },
            |d| {
                black_box(d.evaluate(black_box(&base), &DetectorInput::default()));
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_evaluate);
criterion_main!(benches);
