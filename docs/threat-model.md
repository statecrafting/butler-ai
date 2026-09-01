# butler-ai threat model

What the product defends against, what it does not, and how each spec's
guarantees map to that. Referenced by specs 005, 015, and 017.

## 1. Assets

1. **The user's screen content** and everything derived from it (frames,
   recognized text, prompts, answers). The most sensitive asset: it is
   whatever the user happens to be looking at.
2. **The provider credential** (API key).
3. **The overlay's invisibility to capture**: the product promise.
4. **The integrity of the shipped binary**: an overlay that hides from capture
   is exactly what malware would want to be; a tampered Butler is dangerous.

## 2. Actors

- **Another participant of a screen share or recording** using Zoom, Teams,
  Meet, Slack, OBS, QuickTime, the OS snipping tool, or a browser's
  `getDisplayMedia`.
- **The provider** the user configured (sees redacted text and answers by
  design).
- **Software on the same machine with the user's privileges** (a keylogger, a
  screen recorder below the compositor, a malicious browser extension reading
  the DOM of other apps: not applicable to a webview it does not control).
- **A network observer** between the machine and the provider.
- **Supply-chain actors**: a compromised dependency, a stolen signing key.

## 3. Capture exclusion: what it does and does not defend

| Capture path | Windows (`WDA_EXCLUDEFROMCAPTURE`) | macOS (`sharingType = .none`) | Butler's behavior |
|---|---|---|---|
| Compositor-based: DXGI desktop duplication, `PrintWindow`, `BitBlt` of the screen; ScreenCaptureKit, `CGWindowListCreateImage`, `CGDisplayStream` | excluded (Win10 2004+) | excluded on current macOS; **verified at runtime** because Apple's behavior has changed across releases | self-test on every arm (spec 005 §3.4); `Verified` gates arming |
| Older Windows (< 2004) | only `WDA_MONITOR` (black box) | n/a | reported `Unsupported`; never falls back to the black box (spec 005 §3.2) |
| Remote-desktop protocols that read the frame buffer below the compositor (some RDP/VNC servers, KVM-over-IP) | not excluded | not excluded | **not defended**; documented; the self-test cannot see this path |
| Hardware capture (HDMI capture card, a phone camera pointed at the screen) | not excluded | not excluded | **not defended** by design |
| Accessibility APIs reading UI text (UIA, AX) | not affected by display affinity | not affected by sharing type | the overlay is a webview; AX exposure is the platform's default; **not defended** in v1 |
| Process enumeration, task manager, endpoint agents | visible | visible | **not defended**: Butler is a normal, signed application and does not hide its own existence |

The self-test (spec 005) turns the first row from an assumption into a
measurement. Everything below it is out of scope and the overlay's status
strip never claims otherwise.

## 4. Data handling

The table in spec 015 §3.1 is normative; it is reproduced in
`docs/architecture.md` §4. The enforcement points:

| Threat | Control | Spec |
|---|---|---|
| Frames written to disk (crash, debug, feature creep) | `Frame: !Serialize`, private pixels, no image-format conversion, zeroize on drop, compile-fail tests | 006, 015 |
| Recognized text leaks via logs or errors | `Recognized: !Serialize`; error variants carry kinds only; `field!` log-key allowlist | 007, 015, 016 |
| Secrets in prompts sent to the provider | `redact` is the sole constructor of `RedactedText`; `InferenceRequest` cannot take a `String` | 015, 010 |
| Prompt injection from screen content | screen text is framed as untrusted data; the assistant has no tools and no side effects | 010 §3.3 |
| Exfiltration to an unexpected host | one host per provider; `https://` only; endpoint override surfaced in the UI and logged as a kind; webview has no `connect-src`; egress test | 015 §3.3, 004 §3.2, 012 |
| Credential on disk | OS keychain only; `Secret` has no `Debug`/`Display`/`Clone` | 010 §3.4 |
| Credential in the IPC layer | `StoreSecret` zeroizes its buffer; never logged | 011 §3.2 |
| Over-broad webview capabilities | least-privilege `capabilities/`; constrained by 015; in CODEOWNERS and the `desktop-security` index slice | 004 §3.6, 015, 003 |
| Network observer | TLS via `rustls`; no proxy auto-detection unless enabled | 010 §3.2 |

## 5. The pipeline reading its own output

If the overlay were captured by Butler's own `ScreenSource`, its previous
answer would be OCR'd, detected as a change, and sent back to the model.
Because the capture path is the compositor path and exclusion is verified,
`Verified` means this cannot happen. When status is anything else, the change
detector receives the last rendered answer as an exclusion set (spec 005 §3.6,
008 §3.2 step 2). The guard is applied only when needed so a broken exclusion
cannot hide behind it in tests.

## 6. Supply chain (spec 017)

- Every dependency is pinned exact in `Cargo.lock` / `pnpm-lock.yaml`;
  `cargo deny` enforces licenses, advisories and a source allowlist;
  Dependabot proposes bumps that self-waive the coupling gate only when they
  are version-pin-only.
- Releases are built on GitHub-hosted runners from a tag, signed
  (Authenticode; Developer ID + notarization), attested with build provenance,
  and shipped with SHA-256 sidecars and SBOMs.
- The updater is off by default; when on it verifies the manifest signature
  with an embedded key and installs only after the user confirms.
- Signing-key custody and rotation: to be recorded here before the first
  release (spec 017 AC-2).

## 7. Platforms

Windows 10 2004+ and Windows 11; macOS 13+. Linux is deferred: X11 has no
compositor-level exclusion, and Wayland's screencast portal behavior varies by
compositor, so the product promise cannot be verified there yet (spec 001 §6).

## 8. Governance threats (the corpus itself)

| Threat | Control |
|---|---|
| Code merged that no spec claims | `require_ownership = true` (`C-002`) and `index coverage --fail-on-untraced` in CI; unamendable anchor `ownership-ratchet` |
| An agent edits a spec to make the gate pass | the refusal rule (`adversarial-prompt-refusal.md`), the coherence-guard halts in `/ship` and `/shepherd`, human-only `status: approved` |
| Stale derived artifacts | `compile --check` and `index check` in CI; hooks recompute locally; the merge driver regenerates on conflict |
| A change to what runs with tokens | `on`/`permissions`/job keypaths are section units owned by specs 003 and 017; CODEOWNERS on `.github/` |
| A change to what agents may do | `.claude/settings.json` and the skills/agents/rules are claimed, hashed, and gated (spec 002) |
