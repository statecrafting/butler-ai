// Spec: specs/012-overlay-ui/spec.md
//
// A fixture, not product code. Every line here is a Solid mistake that
// typechecks, renders once, and then silently stops updating: destructured
// props, a conditional early return from the component body. FR-006 lints
// this file and asserts `eslint-plugin-solid` reports them.

import { createSignal } from "solid-js";

export function Destructured(props: { readonly label: string }) {
  // solid/no-destructure: `label` is read once and never updates again.
  const { label } = props;
  return <span>{label}</span>;
}

export function EarlyReturn(props: { readonly ready: boolean }) {
  // solid/components-return-once: the component body runs once, so this
  // branch is decided forever at mount.
  if (!props.ready) {
    return <span>waiting</span>;
  }
  const [count] = createSignal(0);
  return <span>{count()}</span>;
}
