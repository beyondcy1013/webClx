# Mobile Terminal Touch Selection

Date: 2026-05-16

## Root Cause

The terminal touch-selection path treated a slow drag as text selection. Once a touch moved more than the drag threshold after 280ms, `static/terminal.js` synthesized xterm mouse selection even if the user had not long-pressed.

## Stable Rule

Touch text selection is allowed by default so long-press copy is available immediately.

Touch text selection must require a long press first. Movement before the long-press threshold should cancel the selection candidate so normal touch dragging and scrolling remain usable.

When the long press threshold is reached, create a visible one-cell terminal selection immediately. This is what makes the copy button and draggable selection handles appear even before the user moves their finger.

After that initial long press, ordinary movement of the original touch must not expand the selection. Keep the one-cell selection stable so the page does not start scrolling or selecting unpredictably. Selection expansion belongs to dragging the two visible selection handles.

For copying dynamic terminal output without disturbing drag behavior, use the mobile `全能` menu action `新窗口复制`. It opens the current visible terminal text in a plain browser textarea in a new window so the user can manually select/copy there instead of relying on xterm selection.

If long-pressing terminal text shows four handles or flickers, check whether browser-native text selection is active at the same time as the custom xterm selection. The fix is to suppress native `selectstart`/`contextmenu` while a terminal touch-selection candidate or selection is active, clear DOM selection ranges before selecting in xterm, and keep only the two custom `.terminal-selection-handle` controls visible.

## Implementation

- `static/terminal-touch-selection-policy.js` owns the touch-selection timing policy.
- `static/terminal.js` uses that policy to create the initial long-press selection. The initial touchmove after long press is swallowed without changing the selected range; the separate handle-drag path expands or shrinks the range.
- `static/terminal.js` also guards touch-selection startup against browser-native selection so mobile browsers do not show native copy handles on top of the xterm handles.
- `static/terminal.html` loads the policy before `terminal.js`.
- `src/main.rs` embeds the policy asset for static fallback serving.

## Verification

```bash
node tests/terminal-touch-selection-policy.test.mjs
node --check static/terminal.js
node --check static/terminal-touch-selection-policy.js
cargo test embedded_assets_include_page_dependencies -- --nocapture
```

After frontend-only fixes, verify `/home/bin/webclx/static/terminal.js` is synced with `static/terminal.js`; otherwise the running terminal page will still use the old touch-selection behavior.
