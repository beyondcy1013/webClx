# Terminal Rename Presets

Terminal rename preset buttons are configured through `/api/settings` as `terminal_rename_presets`.

Stable conclusions:

- Defaults are `完结` and `复用对话`.
- Settings page System tab stores one preset per line in `#terminal-rename-presets-input`.
- Both the workspace-history terminal list on `/sessions` and the independent
  terminal management page on `/terminal` use `#terminal-rename-dialog` for
  renaming. The independent page retains its preset buttons in
  `#session-rename-presets`; it must not fall back to the old inline
  `#session-rename-inline` editor.
- Clicking a preset appends `_<preset>` to the current rename input; saving still uses the existing terminal session rename API.
- Opening terminal rename appends a draft `_` and places the cursor after it. Submission trims whitespace and removes every trailing `_` while preserving underscores inside the name; the backend applies the same normalization as a final guard.
- While the rename dialog is open, terminal auto-focus paths must not refocus xterm. Quick-start and reconnect flows call helpers such as `focusTerminalSoon()` / `focusTerminalIfAllowed()`, so the guard belongs in `static/terminal-focus-selection.js`, not in the rename input handler alone. Escape, backdrop click, and Cancel close the dialog and restore focus to the rename trigger.
- Backend persistence lives in `settings_core`; frontend files must still be synced to `/home/bin/webclx/static/` after static changes.

Common files:

- `crates/settings_core/src/lib.rs`
- `crates/settings_core/src/storage.rs`
- `crates/settings_core/src/manager.rs`
- `crates/settings_core/src/api.rs`
- `static/index.html`
- `static/app.js`
- `static/terminal.html`
- `static/terminal.js`
- `static/styles.css`
