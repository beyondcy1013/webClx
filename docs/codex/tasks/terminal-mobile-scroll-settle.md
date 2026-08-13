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

## Verification

```bash
node --check static/terminal.js
node tests/terminal-layout-scroll-preserve.test.mjs
node tests/terminal-session-switch-output.test.mjs
node tests/terminal-backlog-replay.test.mjs
```
