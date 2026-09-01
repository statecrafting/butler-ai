---
paths:
  - "apps/desktop/src/**"
  - "apps/desktop/*.{ts,js,json,html}"
---

# Overlay frontend (butler-ai)

- SolidJS + TypeScript strict + Vite (spec 012). No CSS framework, no web
  fonts, no remote URLs anywhere in `src/` or `dist/`.
- The UI is a renderer of runtime state. It makes no capture, inference, or
  timing decisions (pacing is Rust-side, spec 013).
- IPC: import only from `src/generated/bindings.ts` (generated from
  `crates/butler-core/src/ipc.rs`; regenerate with `cargo run -p
  butler-desktop --bin export-bindings`; never hand-edit). `@tauri-apps/api`
  is imported only in `src/ipc/client.ts` (ESLint enforces both).
- Transparency: `html, body, #root` stay `background: transparent`;
  `body { pointer-events: none }`; interactive elements opt in only while
  `state.interactive` is true.
- The degraded banner text is fixed ("VISIBLE TO SCREEN SHARING") and cannot
  be dismissed while degraded (spec 005 §3.5).
- Every `.tsx`/`.ts` file opens with `// Spec: specs/NNN-slug/spec.md`.
- Verify: `pnpm -r typecheck && pnpm -r lint && pnpm -r test`.
