// Spec: specs/012-overlay-ui/spec.md

import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Spec 012 §3.1. Two settings here are contract rather than preference.
//
// `base: "./"` keeps every asset reference relative, because Tauri serves the
// build from a custom protocol rather than from a web root; an absolute `/`
// path resolves to nothing there.
//
// `build.outDir` is `dist`, which is what `tauri.conf.json`'s
// `build.frontendDist` (`../dist`) points at. Spec 004 D-3 has `build.rs`
// write a placeholder there so the Rust crate can compile before this package
// existed; a real build overwrites it.
export default defineConfig({
  plugins: [solid()],
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // FR-005: no external network references in `dist/`. Inlining assets and
    // emitting no source maps keeps the output to files the webview reads
    // from disk, with nothing pointing outside it.
    sourcemap: false,
    target: "esnext",
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  test: {
    globals: true,
    environment: "jsdom",
    // FR-001 asserts a *computed* background, so the stylesheets have to be
    // real in the test DOM rather than stubbed away as empty modules.
    css: true,
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    // vite-plugin-solid needs the browser condition to resolve Solid's
    // client build; without it Vitest pulls the server renderer and every
    // component test sees an empty DOM.
    server: { deps: { inline: [/solid-js/, /@solidjs/] } },
  },
});
