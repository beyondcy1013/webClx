# Windows Compatibility Design

Date: 2026-05-13

## Goal

Make `webClx` support native Windows compilation and basic runtime behavior while preserving the existing Linux behavior.

## Scope

- Linux behavior remains the primary production path: `tmux`, `systemd`, `/home` workspace defaults, and current deployment rules stay intact.
- Windows native runtime gets basic file browsing, settings, auth/preset APIs, and browser terminal sessions backed by PowerShell through `portable-pty`.
- Windows does not try to emulate Linux `tmux` persistence in this first pass. Sessions are process-local and are expected to end when the `webclx` process exits.
- Linux-only system management APIs degrade on Windows with clear messages instead of assuming `systemctl`, `journalctl`, or `/etc/default/webclx`.

## Design

- Add target-specific platform helpers where Unix-only behavior currently leaks into shared code.
- Resolve non-Unix current user profile from `USERNAME`/`USERPROFILE`/`USER`/`HOME`, with PowerShell as the default shell on Windows.
- Make workspace defaults platform-specific: Linux keeps `/home/codes` under `/home`; Windows uses the current user's home directory as the workspace limit.
- Keep terminal management API shapes unchanged. On Unix it continues to create/attach `tmux`; on Windows it creates stored sessions without `tmux` and attaches by spawning PowerShell directly.
- Disable startup tool bootstrap on Windows for now because the current installer downloads Linux Node.js tarballs through POSIX shell tools.

## Verification

- Add unit tests for platform workspace defaults and env-based non-Unix user fallback.
- Run `cargo test --workspace`.
- Attempt `cargo check --workspace --target x86_64-pc-windows-gnu` when the target is available; if not, report the missing target explicitly.
