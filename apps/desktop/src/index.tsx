// Spec: specs/012-overlay-ui/spec.md

/** The overlay's entry point. Mount, and nothing else. */

import { render } from "solid-js/web";

import { App } from "./App";
import "./styles/base.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("spec 012: index.html must provide #root");
}

render(() => <App />, root);
