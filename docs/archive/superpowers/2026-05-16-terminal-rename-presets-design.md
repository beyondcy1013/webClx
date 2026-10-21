# Terminal Rename Presets Design

## Goal

Add configurable terminal rename presets so the rename editor can quickly append status suffixes such as `_完结` or `_复用对话`.

## Behavior

- Settings exposes a `terminal_rename_presets` list.
- Default presets are `完结` and `复用对话`.
- The settings page System tab shows a multiline editor, one preset per line.
- The terminal rename editor renders one button per preset when rename mode is open.
- Clicking a preset appends `_<preset>` to the current rename input and keeps focus in the input.
- Duplicate, blank, and control-character preset names are ignored.

## Architecture

- `settings_core` owns defaults, sanitization, persistence, API response, and API save handling.
- `static/app.js` loads/saves the list through the existing settings endpoint.
- `static/terminal.js` loads the same setting and renders preset buttons inside the existing inline rename panel.

## Testing

- Rust settings tests cover defaults and sanitization.
- Node static tests cover settings UI wiring and terminal rename preset behavior.
