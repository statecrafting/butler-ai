// Spec: specs/012-overlay-ui/spec.md

/**
 * The one subscription (spec 012 §3.4).
 *
 * `butler://event` carries every `UiEvent`, tagged, so this file subscribes
 * once and hands each payload to the store. Nothing else in the overlay
 * listens, and no component calls IPC directly.
 */

import { subscribe } from "../ipc/client";
import { apply } from "./runtime";

/**
 * Start the subscription. Returns a function that stops it.
 *
 * Called once from `App`'s mount. It is `async` because the Tauri listener
 * registration is, and the caller stores the resulting unlisten so an unmount
 * during development's hot reload does not leave two subscriptions folding
 * the same events into the store twice.
 */
export async function connect(): Promise<() => void> {
  const unlisten = await subscribe(apply);
  return unlisten;
}
