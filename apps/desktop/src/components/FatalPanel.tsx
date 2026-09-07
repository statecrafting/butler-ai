// Spec: specs/012-overlay-ui/spec.md

/**
 * The contract-version mismatch panel (spec 012 §3.3, spec 011 §3.4).
 *
 * Only reachable in development. A release ships the binary and the generated
 * bindings from one commit, so the majors cannot differ; in development they
 * can, and the symptom without this panel is an overlay that renders nothing
 * and reports nothing.
 */

export function FatalPanel(props: { readonly running: string }) {
  return (
    <div class="plate interactive" role="alert">
      <strong>Rebuild required.</strong>{" "}
      <span class="dim">
        This overlay was generated against a different IPC contract major than
        the running process ({props.running}). Run{" "}
        <code>cargo run -p butler-desktop --bin export-bindings</code> and
        rebuild the frontend.
      </span>
    </div>
  );
}
