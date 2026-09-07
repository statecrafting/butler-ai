// Spec: specs/012-overlay-ui/spec.md

/**
 * The one file allowed to import `@tauri-apps/api` (spec 012 §3.4).
 *
 * Everything the overlay knows about the Rust process comes through here, and
 * every type it uses comes from `../generated/bindings`, which spec 011
 * generates from `crates/butler-core/src/ipc.rs`. A component that imported
 * the Tauri API directly, or that hand-wrote an IPC payload type, would be a
 * second seam beside the one spec 011 exists to make single. ESLint's
 * `no-restricted-imports` rule enforces the first half and `src/test/` proves
 * the rule is wired (FR-004).
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import {
  commands,
  EVENT_CHANNEL,
  IPC_CONTRACT_VERSION,
  type UiEvent,
} from "../generated/bindings";

export type { UiEvent };
export { commands, IPC_CONTRACT_VERSION };

/**
 * Subscribe to the one event channel (spec 011 §3.2).
 *
 * There is a single channel carrying a tagged payload, so the overlay
 * subscribes once and switches on `type`. Returns the unlisten function.
 */
export async function subscribe(
  onEvent: (event: UiEvent) => void,
): Promise<UnlistenFn> {
  return listen<UiEvent>(EVENT_CHANNEL, (message) => {
    onEvent(message.payload);
  });
}

/**
 * Whether the bindings this bundle was built from match the running process
 * (spec 011 §3.4).
 *
 * Only the major is compared: a minor difference is additive by the
 * `constrains` edge on `ipc.rs`, so an older UI still understands a newer
 * process. A major difference means the two were built from different
 * contracts, which a release cannot produce (the binary and the bindings ship
 * from one commit) and development can.
 */
export async function contractMatches(): Promise<boolean> {
  const running = await commands.getContractVersion();
  return running[0] === IPC_CONTRACT_VERSION[0];
}
