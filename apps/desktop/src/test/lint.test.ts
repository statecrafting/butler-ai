// Spec: specs/012-overlay-ui/spec.md

/**
 * FR-004 and FR-006: the lint rules are wired, not merely written.
 *
 * A rule that is configured but never reached is worse than no rule, because
 * it reads as protection. These tests run ESLint over fixtures built to
 * violate the two rule groups §3.1 calls load-bearing, and assert the
 * violations are reported by name and at `error`.
 *
 * The fixtures are excluded from the ordinary lint run (`eslint.config.js`
 * ignores `src/test/fixtures/`), because they are supposed to fail. The last
 * test here asserts that exclusion too, so the two facts cannot drift: the
 * fixtures must be ignored by `pnpm lint` **and** must still violate when
 * linted directly.
 */

import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";

// `process.cwd()` rather than a path derived from `import.meta.url`: Vite
// rewrites module URLs to its `/@fs/...` form, which ESLint cannot resolve.
// Vitest runs with the package directory as the working directory.
const PACKAGE_ROOT = process.cwd();

const FORBIDDEN_IMPORT = "src/test/fixtures/forbidden-import.ts";
const SOLID_REACTIVITY = "src/test/fixtures/solid-reactivity.tsx";

/** Lint a fixture with the project's real config, ignores lifted. */
async function lintFixture(file: string) {
  const eslint = new ESLint({ cwd: PACKAGE_ROOT, ignore: false });
  const [result] = await eslint.lintFiles([file]);
  return result?.messages ?? [];
}

describe("FR-004: the Tauri API is reachable from one file only", () => {
  it("reports an import of @tauri-apps/api outside src/ipc/client.ts", async () => {
    const messages = await lintFixture(FORBIDDEN_IMPORT);
    const offence = messages.find((m) => m.ruleId === "no-restricted-imports");

    expect(offence, "no-restricted-imports did not fire").toBeDefined();
    expect(offence?.severity).toBe(2);
  });
});

describe("FR-006: eslint-plugin-solid is at error and reaches components", () => {
  it("reports the reactivity mistakes that typecheck and render once", async () => {
    const messages = await lintFixture(SOLID_REACTIVITY);
    const solid = messages.filter((m) => (m.ruleId ?? "").startsWith("solid/"));

    expect(solid.length, "no solid/* rule fired on the fixture").toBeGreaterThan(0);

    const rules = new Set(solid.map((m) => m.ruleId));
    // Destructured props are reported as a reactivity loss: the value is read
    // once, at mount, and never tracked again.
    expect(rules).toContain("solid/reactivity");
    // A conditional early return decides the branch forever at mount.
    expect(rules).toContain("solid/components-return-once");

    // §3.1 puts the set at `error`, because a warning here is a component
    // that has already silently stopped updating.
    expect(solid.every((m) => m.severity === 2)).toBe(true);
  });
});

describe("the fixtures are excluded from the ordinary lint run", () => {
  it("is ignored by pnpm lint, so purpose-built bad code does not fail it", async () => {
    const eslint = new ESLint({ cwd: PACKAGE_ROOT });
    const results = await eslint.lintFiles([FORBIDDEN_IMPORT, SOLID_REACTIVITY]);

    for (const result of results) {
      const reported = result.messages.filter((m) => m.ruleId !== null);
      expect(reported, `${result.filePath} was linted, not ignored`).toEqual([]);
    }
  });
});
