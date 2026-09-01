---
id: "007-text-recognition"
title: "Text recognition: native on-device OCR with a normalized text output"
status: draft
kind: "feature"
domain: "pipeline"
created: "2026-09-01"
implementation: pending
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
