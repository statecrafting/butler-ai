// Spec: specs/012-overlay-ui/spec.md
//
// A fixture, not product code. It exists to be linted by
// `src/test/lint.test.ts` and to fail, which is how FR-004 proves the
// `no-restricted-imports` rule is actually wired rather than merely written.
// `eslint.config.js` ignores this directory for the ordinary lint run.

import { invoke } from "@tauri-apps/api/core";

export function reachAroundTheContract(): Promise<unknown> {
  return invoke("arm");
}
