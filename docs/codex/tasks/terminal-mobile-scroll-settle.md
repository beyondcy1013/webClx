# Mobile Terminal Scroll Settle Window

Date: 2026-06-04

## Symptom

On mobile, the terminal could fail to stay at the bottom. During system IME focus or visual viewport changes, it could visibly jump upward and then scroll back down.

## Root Cause

`fitTerminal()` preserved scroll position only through the next animation frame. Mobile browser `visualViewport` and system IME animations can continue adjusting xterm's `.xterm-viewport` scroll position after that frame. Those delayed layout-induced scroll events were saved as the user's session scroll position. Output written during the same transient window could then decide it was no longer at the bottom.

When ordinary typing was used instead of paste, the terminal did not run the same short post-input visibility refresh. After a few characters, later viewport or IME adjustments could leave the active input line below the visible area even though the PTY input had already been sent.

## Fix Pattern

- Keep the existing immediate and next-frame scroll restoration around terminal layout fitting.
- Add a short layout scroll-save suppression window for delayed mobile viewport/IME corrections.
- While suppression is active, decide output stick-to-bottom behavior from the saved session bottom state rather than the transient viewport scroll position.
- Preserve the page-level bottom position around `resize` and `visualViewport` layout refreshes. Closing the system IME can change the visual viewport after the terminal is already at the bottom; if the page was at its bottom before the layout refresh, restore the page to the new bottom immediately and during the same settle window.
- Capture the page-bottom snapshot at the first event in a `visualViewport` resize sequence, fit once on the next animation frame, and reuse that snapshot for the debounced final fit. Waiting until the debounce expires leaves the old terminal height visible during the keyboard animation, which exposes earlier rows before the terminal returns to the bottom.
- Apply the same short visibility refresh window after ordinary terminal input as after paste so the active input line stays visible while mobile viewport and IME changes settle.
- Treat app/page resume separately from user input. Focus, `visibilitychange`, and Android IME resize events must preserve the terminal scroll snapshot captured before layout refresh; they must never call `scrollTerminalToBottom()` or force-focus xterm. Only explicit user input and live-output follow mode may move the terminal to the bottom.

## Regression Gate

`tests/terminal-layout-scroll-preserve.test.mjs` locks the resume path to the
snapshot-preservation contract and rejects any future reintroduction of forced
bottom scrolling or terminal focus. Keep this test in the full
`node --test tests/*.test.mjs` pre-submit baseline.

System IME focus has one authorized entry point: the `系统键盘` checkbox inside
the `全能` menu. Every other soft key, menu trigger, and menu item must block
native focus changes and restore xterm's helper textarea to its pre-gesture
focus state after dispatch: an open system keyboard stays open, while a closed
one stays closed.
The command layer must also reject `show_system_keyboard` unless that checkbox
is the recorded source. `tests/terminal-ime-policy.test.mjs` and
`tests/terminal-ime-policy.browser.py` lock all three gates, including the
`项目管理` menu and `visualViewport` height.

Dialogs opened from command buttons follow the same rule. In particular,
`定时` may reveal the scheduled-paste panel but must initially focus a
non-editable control; it must not call `focus()` on the paste textarea, delay
number input, or datetime input.

This is enforced as a page-wide capture gate, not a per-button allowlist. Any
button-like action blocks editable focus, including focus caused later by
asynchronous handlers or browser defaults. The block remains until a direct
user gesture on an editable field; the only action-control exception is the
`全能 > 系统键盘` checkbox.

The terminal content surface is a direct-input gesture, not an action control.
`focusTerminalFromTerminalTap()` must clear the action gate before asking xterm
to focus, so tapping the input area can still open the system keyboard when the
normal IME policy permits it.

Never restore an xterm helper focus snapshot by calling `focus()` after a
button action. Android may leave the helper as `activeElement` after the user
manually dismisses the keyboard; a later `focus()` on that same helper reopens
the keyboard. Ordinary controls preserve focus only by cancelling their native
pointer focus before dispatch.

In the Android APK, browser focus and `visualViewport` are not authoritative
for IME visibility. `MainActivity` tracks the actual obscured window area and
exposes `WebClxAndroid.isSystemKeyboardVisible()`. Ordinary actions consult
that native state first: when the IME is closed they synchronously blur stale
xterm focus; when it is open they leave the focus unchanged.

## Verification

### 2026-10-25 Android soft-key IME recurrence

The no-refocus rule alone did not cover all native IME triggers.
`syncTerminalImePolicy()` rewrote six textarea attributes on every pointer,
touch and delayed restore, even when their values were unchanged. Updating
input attributes on a focused WebView textarea can restart its native input
connection. Synchronization now writes only changed attributes.

The October 14 fix (`0a9c1ffa`) also exempted soft keys from clearing stale
helper focus when native Android reported the keyboard closed. Its soft-key
snapshot used helper focus OR native visibility, allowing stale focus to
override an explicit native `false`. The capture gate now clears stale focus
for soft keys too, and snapshots use the common native-first visibility helper.
An already-visible keyboard retains focus; these changes never refocus xterm.

Earlier broad button coverage stubbed `triggerMobileKey`, and the dismissed
keyboard case explicitly blurred the helper during setup. Those checks missed
both repeated attribute writes and Android Back dismissal retaining focus.
`tests/terminal-ime-policy.browser.py` now additionally taps real Home/Tab/End
buttons, checks exact WebSocket bytes, zero helper focus calls and zero IME
attribute mutations. It covers open, closed and closed-with-stale-focus states
under both canvas and Android WebView UA/DOM rendering. Native visibility is
simulated through the actual bridge lookup; it is not a device IME test.

Verification: the new behavioral assertion fails on the old implementation's
attribute writes and passes after the fix in both renderers. IME policy,
session activity/details Node checks and document/dialog Chinese-composition
browser checks pass. The latter needed one rerun after a WebSocket assertion
race. `terminal-cursor-guard.test.mjs` still fails on its pre-existing hardcoded
stylesheet version (`20260926-workflow-history`, while HEAD loads
`20261021-account-sort-all`); that failure is unrelated to these changes.

Static loaders use `20261025-ime-state`; live HTTP SHA-256 matches both changed
JS files. The Android client loads server pages, so a page reload applies the
fix without rebuilding the APK. Device verification remains tracked as R7 in
`todo.md`: native IME hidden/open events must remain unchanged when tapping
ordinary soft keys, including immediately after Back dismisses the keyboard.

```bash
node --check static/terminal.js
node tests/terminal-layout-scroll-preserve.test.mjs
node tests/terminal-session-switch-output.test.mjs
node tests/terminal-backlog-replay.test.mjs
```
