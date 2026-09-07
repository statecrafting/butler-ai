// Spec: specs/012-overlay-ui/spec.md

import js from "@eslint/js";
import solid from "eslint-plugin-solid/configs/typescript";
import globals from "globals";
import tseslint from "typescript-eslint";

/**
 * Spec 012 §3.1.
 *
 * Two rule groups here are load-bearing rather than hygiene.
 *
 * `eslint-plugin-solid` at `error`: Solid components run **once**. React
 * idioms (destructured props, a conditional early return from the component
 * body, a signal read captured into a plain variable at the top level)
 * typecheck, render correctly the first time, and then silently stop
 * updating. There is no runtime error and no failing shallow test. The plugin
 * is what turns that class into a lint failure (FR-006).
 *
 * `no-restricted-imports` on `@tauri-apps/api`: §3.4 says every IPC call goes
 * through `src/ipc/client.ts`, which wraps the generated bindings. A
 * component reaching for the Tauri API directly would be a second, untyped
 * seam beside the one spec 011 exists to make single (FR-004).
 *
 * `src/test/fixtures/` is excluded because the files in it are *supposed* to
 * violate these rules; `src/test/lint.test.ts` runs ESLint over them and
 * asserts the violations are reported. Linting them normally would fail the
 * build on purpose-built bad code.
 */
export default tseslint.config(
  {
    ignores: ["dist/**", "src/generated/**", "src/test/fixtures/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
      globals: { ...globals.browser },
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    ...solid,
    rules: {
      ...solid.rules,
      // §3.1: every rule the plugin's TypeScript config *enables* becomes an
      // error, because a warning in this set is a component that has already
      // silently stopped updating.
      //
      // Rules the plugin deliberately leaves off stay off. `no-proxy-apis` is
      // the one that matters: it exists for environments without `Proxy`, and
      // promoting it would forbid `createStore`, which §3.4 requires.
      ...Object.fromEntries(
        Object.entries(solid.rules ?? {})
          .filter(([, level]) => level !== "off" && level !== 0)
          .map(([rule]) => [rule, "error"]),
      ),
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "@tauri-apps/api",
              message:
                "Spec 012 §3.4: IPC goes through src/ipc/client.ts, which is the only file allowed to import the Tauri API.",
            },
          ],
          patterns: [
            {
              group: ["@tauri-apps/api/*"],
              message:
                "Spec 012 §3.4: IPC goes through src/ipc/client.ts, which is the only file allowed to import the Tauri API.",
            },
          ],
        },
      ],
    },
  },
  {
    // The one exemption §3.4 names.
    files: ["src/ipc/client.ts"],
    rules: { "no-restricted-imports": "off" },
  },
  {
    files: ["vite.config.ts"],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    // This file is JavaScript and is not in `tsconfig.json`'s `include`, so
    // the typed rules have no program for it. Linting it untyped is the point:
    // it is configuration, and the alternative is turning on `allowJs` for the
    // whole package to type-check one config file.
    files: ["eslint.config.js"],
    languageOptions: { globals: { ...globals.node } },
    ...tseslint.configs.disableTypeChecked,
  },
);
