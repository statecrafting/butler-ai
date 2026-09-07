# butler-desktop: manual verification checklist

Spec 004 AC-3. These are the requirements that cannot be checked from a test
process: they need a window server, a real user session, and in two cases a
second application to click on. **Spec 004 does not flip to
`implementation: complete` until this checklist is signed off on both
platforms** (AC-3), which is why it is a file rather than a paragraph in a PR.

Run `cargo run -p butler-desktop` from the repository root.

Record the date, the OS version, and the tester's initials in the sign-off
table. An unchecked box is not a failure; it is an unknown, and the difference
matters.

## macOS

| # | Requirement | What to do | Pass |
|---|---|---|---|
| FR-001 | Window flags | The overlay is transparent, has no title bar, no shadow, and floats above other windows including full-screen apps | [ ] |
| FR-002 | Click-through | With no modifier held, click where the overlay is. The click reaches the window beneath it | [ ] |
| FR-003 | Global shortcuts register | `Cmd+Shift+B`, `Cmd+Shift+H` and `Cmd+Shift+Enter` are absent from the tray's "Shortcuts unavailable" section, so the OS accepted all three. Their effects arrive with specs 019 and 005; there is nothing to observe yet | [ ] |
| FR-003 | Interact toggle | With another app focused, press `Cmd+Shift+Space`: the overlay accepts clicks. Press it again: clicks pass through again. One press is one flip, not two | [ ] |
| FR-004 | No Dock icon | The app has no Dock tile and no app-switcher entry; the menu-bar icon is present | [ ] |
| FR-005 | Permission onboarding | With Screen Recording denied, arming opens the onboarding panel **exactly once per launch** and the pipeline stays disarmed. Arming again does not re-prompt | [ ] |
| §3.4 | Tray menu | Every item is present and enabled; a shortcut that failed to register appears under "Shortcuts unavailable" | [ ] |

## Windows

| # | Requirement | What to do | Pass |
|---|---|---|---|
| FR-001 | Window flags | The overlay is transparent, undecorated, and always on top | [ ] |
| FR-002 | Click-through | With no modifier held, click where the overlay is. The click reaches the window beneath it | [ ] |
| FR-003 | Global shortcuts register | `Ctrl+Shift+B`, `Ctrl+Shift+H` and `Ctrl+Shift+Enter` are absent from the tray's "Shortcuts unavailable" section. Their effects arrive with specs 019 and 005 | [ ] |
| FR-003 | Interact toggle | With another app focused, press `Ctrl+Shift+Space`: the overlay accepts clicks. Press it again: clicks pass through again. One press is one flip, not two | [ ] |
| D-9 | Developer tools still work | In a browser, `Ctrl+Shift+I` still opens developer tools. Butler must not have claimed it globally | [ ] |
| FR-004 | No taskbar button | The overlay has no taskbar button and never takes focus; the tray icon is present | [ ] |
| §3.5 | No permission prompt | Arming never prompts; capture is permitted unconditionally | [ ] |

## Not covered here

- **FR-006** (the webview issues no network request) needs a denying HTTP
  proxy and is a test, not a checklist item. It is not yet written: see
  spec 004 D-6.
- **The effects of arm/disarm, ask now, and toggle visibility.** All three
  register, and the Interact toggle above is the only one wired to anything:
  the first two need the runtime (spec 019), and §3.2 forbids showing the
  overlay before capture exclusion (spec 005) has been applied. FR-003's row
  therefore checks that the OS accepted the bindings, which is all that is
  observable today.

## Sign-off

| Platform | OS version | Date | Tester | All boxes checked |
|---|---|---|---|---|
| macOS | | | | [ ] |
| Windows | | | | [ ] |
