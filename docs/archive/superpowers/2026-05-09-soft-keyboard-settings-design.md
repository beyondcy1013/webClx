# Soft Keyboard Settings Design

## Goal

Add a dedicated settings tab for terminal soft-keyboard behavior, make the existing command controls easier to find, and add a configurable "功能命令" dropdown for terminal UI actions.

## Scope

- Move the existing terminal quick command settings from "设置 > 系统" to a new "设置 > 软键盘" tab.
- Move related soft-keyboard settings into the same tab:
  - desktop browser soft-keyboard visibility
  - soft-keyboard scale
  - terminal quick commands
  - quick-start default command
  - new terminal default environment variables
- Add configurable soft-keyboard function commands.
- Keep existing settings fields compatible so current config files continue to load.

## Data Model

The existing `terminal_quick_commands`, `terminal_quick_start_default_key`, `terminal_default_env_vars`, `desktop_terminal_soft_keyboard_enabled`, and `terminal_soft_keyboard_scale` fields stay unchanged.

Add `terminal_slash_commands`, an ordered list for the existing slash-command dropdown, and `terminal_function_commands`, an ordered list for the new "功能命令" dropdown. Both use the same item shape:

```json
{
  "key": "system_keyboard",
  "label": "弹出系统键盘",
  "action": "show_system_keyboard",
  "command": ""
}
```

`action` is used for built-in UI behavior. `command` is reserved for text commands such as slash commands.

## Defaults

The slash command dropdown defaults to:

- /compact
- /resume
- /status

The function command dropdown defaults to:

- 弹出系统键盘: `show_system_keyboard`
- 禁用系统键盘: `disable_system_keyboard`

## Terminal Behavior

The terminal page renders one "功能命令" dropdown from `terminal_function_commands`.

Selecting "弹出系统键盘" forces system IME mode on and focuses the xterm helper textarea.

Selecting "禁用系统键盘" disables system IME mode and records a suppression deadline 60 seconds in the future. During that window, normal terminal refocus paths do not reopen the system keyboard. Explicitly selecting "弹出系统键盘" clears the suppression and opens it.

Slash command function items reuse the existing slash command send behavior.

## Compatibility

Existing hard-coded slash command dropdown behavior is replaced by configurable `terminal_slash_commands`, while the new `terminal_function_commands` renders as a separate "功能命令" dropdown.

Settings save/load remains backward compatible: missing `terminal_function_commands` falls back to defaults.

## Testing

- Rust settings tests cover default function commands and sanitization.
- JS unit tests cover IME command action selection and 60-second suppression.
- Browser/static verification checks the terminal page loads the new asset versions.
