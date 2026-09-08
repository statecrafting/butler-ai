---
id: "007-text-recognition"
title: "Text recognition: native on-device OCR with a normalized text output"
status: approved
kind: "feature"
domain: "pipeline"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: medium
platforms: ["windows", "macos"]
phase: 3
depends_on:
  - "006-screen-capture"
establishes:
  - { kind: crate, id: "butler-ocr" }
  - "crates/butler-ocr/Cargo.toml"
  - "crates/butler-ocr/src/lib.rs"
  - "crates/butler-ocr/src/recognizer.rs"
  - "crates/butler-ocr/src/recognized.rs"
  - "crates/butler-ocr/src/normalize.rs"
  - "crates/butler-ocr/src/apple_vision.rs"
  - "crates/butler-ocr/src/windows_ocr.rs"
  - "crates/butler-ocr/tests/normalize.rs"
  - { kind: directory, path: "crates/butler-ocr/tests/fixtures/" }
  - { kind: symbol, id: "butler_ocr::recognizer::TextRecognizer" }
  - { kind: symbol, id: "butler_ocr::recognized::Recognized" }
  - { kind: symbol, id: "butler_ocr::normalize::normalize" }
extends:
  # `crates/*` already globs this crate into the workspace, but the OCR
  # backends' bindings are pinned once in spec 001's root manifest
  # (001 FR-004) before this crate's manifest references them.
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
  # The manual checklist: FR-001 needs reference hardware and the Windows
  # engine needs a Windows host to be seen recognizing anything (D-2, D-3).
  # 018 R-010 says such a row is recorded as deferred rather than unchecked.
  - { spec: "012-overlay-ui", unit: "apps/desktop/README.md", nature: additive }
summary: >
  The `butler-ocr` crate: a `TextRecognizer` trait that turns a `FrameView`
  into `Recognized` text with per-line geometry and confidence, backed by the
  operating system's own OCR engine (Apple Vision `VNRecognizeTextRequest` on
  macOS, `Windows.Media.Ocr` on Windows) so no model ships with the app and no
  pixels leave the process; plus a deterministic normalization pass that turns
  engine output into the stable, layout-ordered string the change detector
  (008) compares and the assistant (010) reads. Recognition runs entirely on
  device.
---

# 007: Text recognition

## 1. Purpose

The pipeline reasons about text, not pixels. Both target platforms ship a
capable OCR engine with hardware acceleration and no download, so butler-ai
uses them rather than bundling Tesseract or an ONNX model: smaller binary, no
model licensing, no GPU dependency of our own, and the engines keep improving
with the OS.

Engine output is noisy in ways that would make change detection unstable
(line order varies with layout heuristics, whitespace differs frame to frame,
confidence flickers on anti-aliased text). The normalization pass is the part
of this spec that makes the rest of the pipeline deterministic.

## 2. Territory

The crate `butler-ocr` and its files. `recognizer.rs` defines the trait;
`apple_vision.rs` and `windows_ocr.rs` are the `cfg`-gated engines;
`recognized.rs` the output type; `normalize.rs` the pure normalization (no
platform code; tested on every CI target with the fixtures under
`tests/fixtures/`). On other targets a `NullRecognizer` returns
`Unavailable`.

## 3. Behavior

### 3.1 The trait

```rust
pub trait TextRecognizer: Send + Sync {
    fn recognize(&self, view: FrameView<'_>, opts: &RecognizeOptions) -> Result<Recognized, OcrError>;
}
pub struct RecognizeOptions { pub languages: Vec<LanguageTag>, pub fast: bool, pub min_confidence: f32 }
```

- `recognize` MUST return within 1500 ms or `OcrError::Timeout`. It runs on a
  blocking task owned by the runtime (009).
- `OcrError` variants carry no text or pixels: `Timeout`, `EngineUnavailable`,
  `LanguageUnsupported(LanguageTag)`, `Internal(String)` (message
  sanitized).

### 3.2 `Recognized`

```rust
pub struct Recognized {
    pub lines: Vec<Line>,          // engine order, un-normalized
    pub text: String,              // normalized (§3.4), the pipeline's view
    pub mean_confidence: f32,
    pub engine: EngineId,
    pub recognized_at: Instant,
}
pub struct Line { pub text: String, pub bbox: Rect, pub confidence: f32 }
```

`Recognized` MAY be `Clone` (it is text, and the pipeline holds the previous
one for comparison) but MUST NOT be `Serialize`: it leaves the process only
inside an `InferenceRequest` (010) after redaction (015), never as a whole.

### 3.3 Engines

- **macOS (`apple_vision.rs`)**: `VNRecognizeTextRequest` with
  `recognitionLevel = .accurate` (or `.fast` when `opts.fast`), `usesLanguage
  Correction = true`, `recognitionLanguages` from options, on a `CGImage`
  built from the `FrameView` without copying when the stride allows. Bindings
  via `objc2-vision`.
- **Windows (`windows_ocr.rs`)**: `Windows.Media.Ocr.OcrEngine::
  TryCreateFromLanguage` (falling back to `TryCreateFromUserProfileLanguages`),
  `RecognizeAsync` on a `SoftwareBitmap` (BGRA8) built from the view. Bindings
  via the `windows` crate.
- Both MUST downscale a view whose longest side exceeds the engine's practical
  maximum (4096 px) with a box filter before recognition, recording the factor
  so `bbox` values are reported in frame coordinates.
- Neither engine is given the whole frame when a region of interest is set by
  settings (014); the runtime crops first.

### 3.4 Normalization (`normalize(lines: &[Line], frame: Size) -> String`)

Pure, deterministic, tested against fixtures. It MUST:

1. Drop lines with `confidence < min_confidence` (default 0.5) or shorter
   than two graphemes.
2. Order lines top-to-bottom, then left-to-right, using bbox centre with a
   row-tolerance of half the median line height, so column layouts read in
   reading order and jitter of a few pixels does not reorder lines.
3. Collapse internal whitespace to single spaces; trim; join lines with `\n`;
   NFC-normalize; map typographic quotes and dashes to ASCII; drop zero-width
   characters.
4. Never alter letters or digits: normalization is layout and whitespace
   only, so the assistant sees what the user sees.

The output is the string the change detector compares (008) and the prompt
carries (010). Two frames of the same static screen MUST normalize to the
same string (FR-002).

## 4. Functional requirements

- **FR-001.** `recognize` on a 1080p screenshot of a typical document
  completes in under 400 ms p50 on the reference hardware.
- **FR-002.** For each fixture pair in `tests/fixtures/stable/` (two frames
  of an unchanged screen), `normalize` yields identical strings.
- **FR-003.** For each fixture in `tests/fixtures/columns/`, the normalized
  order matches the expected reading order recorded beside it.
- **FR-004.** The crate makes no network call and links no networking crate:
  `cargo tree -p butler-ocr` contains neither `reqwest`, `hyper`, `tokio-
  tungstenite` nor `ureq`.
- **FR-005.** A view larger than 4096 px on its longest side is downscaled and
  `bbox` coordinates are reported in original frame pixels.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-ocr` passes on both platform runners; the
  `normalize` tests also pass on Linux.
- **AC-2.** Fixture screenshots under `tests/fixtures/` contain no personal
  data: they are synthetic renders generated by `tests/fixtures/gen.rs` from
  lorem-ipsum text, and the generator is committed with them.
- **AC-3.** `docs/architecture.md` records the engine choice and the
  alternatives considered (Tesseract, PaddleOCR, RapidOCR/ONNX) with the
  reasons for rejection.

## 6. Out of scope

- Which region of the screen is recognized (settings, 014) and when (009).
- Language packs: the app uses whatever languages the OS engine has installed
  and surfaces `LanguageUnsupported` to the UI; it never downloads packs.
- Handwriting, math, or non-text content.

## 7. Resolved decisions

- **D-1 (2026-09-07, one resampler, not two).** §3.3 requires both engines to
  downscale a view whose longest side exceeds 4096 px with a box filter. The
  first implementation wrote that filter twice, once per engine, differing only
  in channel order.

  It now lives once in `recognizer.rs`, with `swap_rb` as the whole of the
  difference between the two paths. Two copies would have been two places for a
  resampling bug to live and, worse, only one of them testable on any given
  host: the Windows copy could not be exercised on a macOS runner at all. The
  shared one is covered by tests that run everywhere.

- **D-2 (2026-09-07, the fixtures are `Line` lists, not screenshots).** §3.4's
  rules are about geometry, confidence, whitespace and ordering. A PNG fixture
  would put an OCR **engine** in the middle of a test whose entire purpose is
  to be pure and to run on every target including Linux (§2, AC-1), and a
  failure would then be ambiguous between the engine and the pass under test.

  `tests/fixtures/gen.rs` is committed as AC-2 requires and emits nothing but
  lorem words, which a test asserts. FR-002's pair models what actually
  differs between two frames of a static screen: whitespace the engine reports
  differently, a pixel or two of box jitter, confidence flicker, and engine
  line order.

- **D-3 (2026-09-07, `Windows.Media.Ocr` reports no confidence).** §3.2 gives
  `Line` a `confidence`, and §3.4 rule 1 drops lines below a floor.
  `Windows.Media.Ocr` reports **no confidence at all**, per line or per word.

  Lines from that engine carry `1.0`, and the consequence is stated rather
  than hidden: **§3.4's confidence filter is inert on Windows.** Synthesizing
  a number from word count or box size would be worse, because the filter
  would then appear to work while thresholding something that is not
  confidence, and a user tuning it would be tuning noise.

  The two rules that do the heavy lifting for stability are unaffected: the
  two-grapheme minimum still drops engine noise, and the reading-order pass is
  geometric. **Owed**: if Windows text quality proves a problem in practice, a
  spec amendment naming a different signal, not a guess dressed up as one.

- **D-4 (2026-09-07, what the Windows engine's tests can and cannot be).**
  The Windows path cannot be compiled on this host: cross-compiling the
  workspace to `x86_64-pc-windows-msvc` needs a C toolchain that is not
  present. CI's `windows-latest` runner builds, clippy-checks and tests it on
  every pull request, so a binding that does not compile is caught there.

  What no CI job checks is whether it *recognizes*: the runner has no document
  on a screen. That is a row in `apps/desktop/README.md`, deferred, alongside
  FR-001's latency, which needs the reference hardware §7 of
  `docs/architecture.md` still does not name.

## 8. Verification

```verify:cli
# AC-1: the unit tests and §3.4's rules. The normalize half runs on every
# target including Linux, which is what makes the pass verifiable at all.
cargo test -p butler-ocr --locked
# FR-004: recognition is on device. A dependency that could open a socket
# would be the one place that stopped being true.
sh -c '! cargo tree -p butler-ocr --locked --edges normal | grep -Eq "reqwest|hyper|tokio-tungstenite|ureq"'
# §3.2 and spec 015: `Recognized` may be Clone (the pipeline holds the
# previous one) but never Serialize. It leaves the process only as
# RedactedText inside one InferenceRequest.
#
# Matching a derive or an impl rather than the bare word: the first version
# grepped for "Serialize" and failed on the doc comment that explains the
# rule, which is the trap spec 016 D-2 names. The strongest form of the
# guarantee is the second line: with no serde in the manifest, `Serialize`
# cannot be implemented here at all. Both tested with a negative control.
sh -c '! grep -rnE "^[[:space:]]*(#\[derive.*Serialize|impl .*Serialize)" crates/butler-ocr/src'
sh -c '! grep -q "^serde" crates/butler-ocr/Cargo.toml'
# §3.4 rule 4: normalization is layout and whitespace only. The punctuation
# map is the only character rewriting, and it contains no letter or digit.
grep -q "fn map_punctuation" crates/butler-ocr/src/normalize.rs
sh -c '! grep -A12 "const fn map_punctuation" crates/butler-ocr/src/normalize.rs | grep -qE "=> .[A-Za-z0-9].,"'
# AC-2: the fixture generator is committed and its output is lorem.
test -f crates/butler-ocr/tests/fixtures/gen.rs
# AC-3: the engine choice and its rejected alternatives are in the log.
sh -c 'grep "^| D4 " docs/architecture.md | grep -qi "tesseract"'
# D-1: one resampler, shared. Two would leave the Windows copy untestable on
# any macOS host.
grep -q "pub fn resample_rgba" crates/butler-ocr/src/recognizer.rs
sh -c 'test "$(grep -c "fn resample" crates/butler-ocr/src/*.rs | grep -v ":0" | wc -l | tr -d " ")" -eq 1'
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "007-text-recognition" && exit 1 || exit 0'
```
