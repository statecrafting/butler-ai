# butler-desktop: manual verification checklist

Spec 004 AC-3. These are the requirements that cannot be checked from a test
process: they need a window server, a real user session, and in two cases a
second application to click on. **Spec 004 does not flip to
`implementation: complete` until this checklist is signed off** (AC-3), which
is why it is a file rather than a paragraph in a PR.

Run `cargo run -p butler-desktop` from the repository root.

Record the date, the OS version, and the tester's initials in the sign-off
table. An unchecked box is not a failure; it is an unknown, and the difference
matters.

## How to read the `When` column

Spec 018 **R-010**: a manual checklist row that depends on a capability a
later phase delivers is *deferred*, names the spec it waits on, and is signed
in the phase that delivers it. A deferred row is not an unchecked box. It is a
scheduled one, and that difference is recorded here rather than inferred.

Two capabilities this app does not yet have shape most of this file:

- **The overlay is visible from spec 005 onward.** It was created
  `visible: false` with no code path calling `show()`, because spec 004 §3.2
  forbids showing it before capture exclusion has been applied. Spec 005
  landed that, and `window::show_if_permitted` is now the one path that turns
  it on: only when the self-test reported `Verified`, or when the user
  accepted degraded mode. Rows that ask a tester to look at, click on, or
  click through the overlay are **observable now**, and still need a human.
- **Nothing arms.** Arming is **spec 019**'s runtime, and the onboarding panel
  it would open is spec 012's. Any row about the permission prompt is deferred
  to 019.

## macOS

| # | Requirement | What to do | When | Pass |
|---|---|---|---|---|
| FR-003 | Global shortcuts register | `Cmd+Shift+B`, `Cmd+Shift+H`, `Cmd+Shift+Space` and `Cmd+Shift+Enter` are absent from the tray's "Shortcuts unavailable" section, so the OS accepted all four | phase 2 | [x] |
| FR-004 | No Dock icon | The app has no Dock tile and no app-switcher entry; the menu-bar icon is present | phase 2 | [x] |
| §3.4 | Tray menu | Every item is present and enabled; a shortcut that failed to register appears under "Shortcuts unavailable" | phase 2 | [x] |
| FR-001 | Window flags | The overlay is transparent, has no title bar, no shadow, and floats above other windows including full-screen apps | phase 3, now observable | [ ] |
| FR-002 | Click-through | With no modifier held, click where the overlay is. The click reaches the window beneath it | phase 3, now observable | [ ] |
| FR-003 | Interact toggle | With another app focused, press `Cmd+Shift+Space`: the overlay accepts clicks. Press it again: clicks pass through again. One press is one flip, not two | phase 3, now observable | [ ] |
| FR-005 | Permission onboarding | With Screen Recording denied, arming opens the onboarding panel **exactly once per launch** and the pipeline stays disarmed. Arming again does not re-prompt | deferred to 019 | [ ] |

### Evidence for the phase 2 rows

Verified on macOS 15 (Darwin 25.5.0, arm64) on 2026-09-07 against
`target/debug/butler-desktop`, by probe rather than by eye. Each is
reproducible:

| Row | Probe | Result |
|---|---|---|
| FR-003 registers | run the binary, read stderr | empty. `run()` prints `shortcut unavailable: ...` for every registration the OS refuses, so an empty stream is all four accepted |
| FR-004 no Dock tile | `lsappinfo info -only ApplicationType $(lsappinfo find pid=<pid>)` | `"ApplicationType"="UIElement"`. LaunchServices classifies the process as an accessory: no Dock tile, no app-switcher entry |
| §3.4 tray menu | the process stays up past setup | `build_tray` returns `Err` if the icon or any of the seven menu items fails to build, and `run()`'s setup hook propagates it, so a live process is a fully built menu |

**Not covered by those probes:** that the menu-bar icon *renders* and that its
items read correctly to a human eye. A probe cannot see a menu bar. That
residual check is carried to the same phase 3 pass as the deferred rows above,
when a tester is in front of the running app anyway.

## Windows

| # | Requirement | What to do | When | Pass |
|---|---|---|---|---|
| FR-003 | Global shortcuts register | `Ctrl+Shift+B`, `Ctrl+Shift+H`, `Ctrl+Shift+Space` and `Ctrl+Shift+Enter` are absent from the tray's "Shortcuts unavailable" section | phase 2, deferred: no Windows host | [ ] |
| D-9 | Developer tools still work | In a browser, `Ctrl+Shift+I` still opens developer tools. Butler must not have claimed it globally | phase 2, deferred: no Windows host | [ ] |
| §3.4 | Tray menu | Every item is present and enabled | phase 2, deferred: no Windows host | [ ] |
| FR-001 | Window flags | The overlay is transparent, undecorated, and always on top | phase 3, now observable | [ ] |
| FR-002 | Click-through | With no modifier held, click where the overlay is. The click reaches the window beneath it | phase 3, now observable | [ ] |
| FR-003 | Interact toggle | With another app focused, press `Ctrl+Shift+Space`: the overlay accepts clicks. Press it again: clicks pass through again. One press is one flip, not two | phase 3, now observable | [ ] |
| FR-004 | No taskbar button | The overlay has no taskbar button and never takes focus; the tray icon is present | phase 3, now observable | [ ] |
| §3.5 | No permission prompt | Arming never prompts; capture is permitted unconditionally | deferred to 019 | [ ] |

The three phase 2 Windows rows are deferred for a different reason from the
rest: they are observable today, but no Windows host is available to observe
them on. Spec 004 D-10 records the maintainer's decision to defer them, and
names spec 017 as the point at which they must be signed, since that is the
spec that ships a Windows artifact.

## Not covered here

- **FR-006** (the webview issues no network request) needs a denying HTTP
  proxy and is a test, not a checklist item. It is not yet written: see
  spec 004 D-6 and D-11.
- **The effects of arm/disarm, ask now, and toggle visibility.** All four
  shortcuts register; none is wired to a behaviour yet. Arm, disarm and ask
  need the runtime (spec 019), and §3.2 forbids showing the overlay before
  capture exclusion (spec 005) has been applied, so toggle-visibility has no
  correct behaviour to have. FR-003's row therefore checks that the OS
  accepted the bindings, which is all that is observable today. `Quit` in the
  tray menu is wired and does end the process.

## Overlay appearance (spec 012 AC-2)

A second checklist, with the same `When` semantics as the tables above
(018 R-010). **Every row here is deferred to spec 005**, and for the reason
that governs this whole file: the overlay is created `visible: false` and
nothing shows it, so there is no way to look at it. These rows are what a
tester should look at on the first run where there *is* something to see.

| # | Requirement | What to look for | When | Pass |
|---|---|---|---|---|
| §3.2 | Dark wallpaper | The plate is legible; the wallpaper reads through it; nothing paints a full-window rectangle | phase 3, now observable | [ ] |
| §3.2 | Light wallpaper | The same, with the light plate | phase 3, now observable | [ ] |
| §3.2 | Over a full-screen app (macOS) | The overlay floats above a full-screen window and follows across spaces | phase 3, now observable | [ ] |
| §3.5 | Contrast | Status and answer text meet WCAG AA at 14 px against the plate, on both wallpapers | phase 3, now observable | [ ] |
| §3.2 | 125% scaling (Windows) | Text and plate scale; nothing clips | phase 3, now observable | [ ] |
| §3.2 | 200% scaling (Windows) | The same at 200% | phase 3, now observable | [ ] |
| §3.2 | Idle repaint | Nothing animates while idle. A capture tool's CPU graph stays flat | phase 3, now observable | [ ] |

What *is* checked today, without a window, is in `pnpm -r test`: FR-001
asserts the computed background of `html`, `body` and `#root` is fully
transparent, and FR-002 asserts the surface is inert by default. Those are
the properties a screenshot would be checking anyway; what the list above
adds is a human eye on legibility, which no assertion covers.

## Screen capture (spec 006)

Three of spec 006's requirements need a real display, and two of those need
the reference hardware `docs/architecture.md` §7 names. They are rows here for
the same reason the rest of this file exists (018 R-010): a requirement whose
evidence is a machine nobody has yet is deferred, not unchecked.

| # | Requirement | What to do | When | Pass |
|---|---|---|---|---|
| FR-001 | Capture latency | `capture` on a 4K monitor completes under 100 ms p50 and 250 ms p99 | deferred: reference hardware | [ ] |
| FR-002 | Source determinism | Two consecutive captures of a genuinely static screen (no clock, no cursor over the display) are byte-identical | deferred: reference hardware | [ ] |
| FR-005 | Permission refusal | With Screen Recording denied on macOS, `capture` returns `PermissionDenied` in under 50 ms and **no system prompt appears**. The shell owns prompting | deferred: a macOS account with the grant revoked | [ ] |

`docs/architecture.md` §7 still says the reference machines are "to be fixed in
phase 3" and names neither. §7.1 records an Apple M1 Max for spec 008's
benchmark, which is one of the two. The Windows machine is unchosen, and
naming one that has not been measured on would be worse than leaving it blank.

## Text recognition (spec 007)

One of spec 007's requirements needs the reference hardware, and one needs a
Windows machine. Recorded here for the reason 018 R-010 gives.

| # | Requirement | What to do | When | Pass |
|---|---|---|---|---|
| FR-001 | Recognition latency | `recognize` on a 1080p screenshot of a typical document completes under 400 ms p50 | deferred: reference hardware | [ ] |
| §3.3 | `Windows.Media.Ocr` produces text | On Windows, recognizing a document screenshot returns lines whose text matches what is on screen | deferred: no Windows host | [ ] |

The Windows engine compiles on CI's `windows-latest` runner every PR, so a
binding that does not build is caught. What no CI job checks is whether it
*recognizes*, because the runner has no document on a screen to recognize.
Spec 007 D-3 records a second Windows-only gap: the engine reports no
confidence, so §3.4's confidence filter is inert there.

## Capture exclusion (spec 005)

The self-test measures this on every launch and the status strip shows what it
found, so most of spec 005 needs no checklist. Two things a machine cannot
check, and one that needs a second computer:

| # | Requirement | What to do | When | Pass |
|---|---|---|---|---|
| FR-003 | Really invisible to a real tool | Share the screen in Zoom, Teams, Meet or OBS with the overlay visible. The overlay is absent from what the other party sees, and from a recording | phase 3, now observable | [ ] |
| §3.4 | The sentinel is not a nuisance | Trigger the self-test from the tray. The magenta and green pattern is on screen for at most a frame or two and does not persist | phase 3, now observable | [ ] |
| FR-001 | Windows read-back | On Windows 10 2004+ or Windows 11, the status strip reports `verified` rather than `unsupported` | deferred: no Windows host | [ ] |

The self-test is the product's own answer to the first row, and it runs
automatically. The row exists because the self-test checks *one* capture path
(the compositor's, through spec 006) and a real conferencing tool is the check
that the path it uses is the same one. `docs/threat-model.md` §3 lists the
paths this mechanism does not defend against at all.

## Sign-off

| Platform | Scope | OS version | Date | Tester | Signed |
|---|---|---|---|---|---|
| macOS | phase 2 rows | macOS 15 (Darwin 25.5.0, arm64) | 2026-09-07 | automated probe, see Evidence | [x] |
| macOS | rows deferred to 005 / 019, including every spec 012 AC-2 row | | | | [ ] |
| Windows | phase 2 rows (no host available, 004 D-10) | | | | [ ] |
| Windows | rows deferred to 005 / 019 | | | | [ ] |
