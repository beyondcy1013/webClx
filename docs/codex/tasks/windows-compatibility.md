# Windows Compatibility Notes

Date: 2026-05-12

## Current Status

`webClx` builds on the current Linux host with `cargo check --workspace` and `cargo test --workspace --no-run`.
First-pass native Windows compatibility has been added for compilation-oriented platform boundaries and basic runtime behavior.

Verified on the current Linux host:

- `cargo check --workspace`
- `cargo test --workspace` (`171 passed`)

Windows target verification (updated 2026-06-25):

- `cargo check --target x86_64-pc-windows-gnu --workspace` passes (42 warnings, all Unix/tmux code dead under `cfg(unix)` — no errors).
- `cargo build --target x86_64-pc-windows-gnu --release` produces `webclx.exe` (PE32+ x86-64), verified via `file`. DLL dependencies are all Windows system libs (`kernel32`, `ws2_32`, `advapi32`, ...), so no MinGW runtime DLL needs to be shipped.

## Cross-Compile Method (x86_64-pc-windows-gnu)

The default shell PATH resolves `cargo`/`rustc` to the system sysroot (`/usr/bin/...`), which lacks the windows-gnu std and fails with `error[E0463]: can't find crate for core`. The webClx toolchain is the rustup-managed one under `/home/root/.rustup` (same setup `stockAlert` uses via `scripts/build-windows.sh`). Pin it explicitly:

```bash
export RUSTUP_HOME=/home/root/.rustup
export CARGO_HOME=/home/root/.cargo
export HOME=/home/root
TOOLCHAIN_BIN="$RUSTUP_HOME/toolchains/stable-x86_64-unknown-linux-gnu/bin"
export PATH="$CARGO_HOME/bin:$TOOLCHAIN_BIN:$PATH"
cd /home/codes/webClx
cargo build --target x86_64-pc-windows-gnu --release
```

Notes:

- This is a cross-compile to a different target; the `webclx-compile-api` / `rebuild-and-deploy.sh` path is for the host Linux deploy and does not apply here — run the cross-compile directly.
- Linker is `x86_64-w64-mingw32-gcc` (already installed under `/usr/bin`).
- With `RUSTUP_HOME=/home/root/.rustup`, root's cargo config has no `target-dir` override, so the product lands in the repo at `target/x86_64-pc-windows-gnu/release/webclx.exe` (NOT `/home/cargo-target`, which is the `beyondcy` user's override).
- Installed Windows targets on this host: `x86_64-pc-windows-gnu`, `x86_64-pc-windows-gnullvm`, `x86_64-pc-windows-msvc`.

## Implemented First Pass

- `crates/runtime_paths_core/src/lib.rs` resolves non-Unix current-user profiles from environment values such as `USERNAME`, `USERPROFILE`, `HOME`, `SHELL`, and `COMSPEC`.
- `crates/settings_core/src/lib.rs` keeps Linux `/home` limits but uses the current user home as the Windows workspace root.
- `src/system.rs` and `src/host.rs` guard Unix-only `libc` calls.
- `src/system.rs` uses `/usr/bin/systemd-run` on Linux for delayed service restart so the Settings restart action does not depend on `PATH`; on Windows/non-Linux it returns a clear unsupported message instead of trying to execute `systemd-run`.
- `src/startup_tools.rs` skips the Linux Node/Codex/Claude bootstrap on Windows.
- `src/terminal/session.rs` starts PowerShell directly with `portable-pty` on Windows.
- `src/terminal/manager.rs` keeps `tmux` on Linux and treats Windows terminal sessions as process-local, non-persistent sessions.
- `src/main.rs` marks Windows release builds with `windows_subsystem = "windows"` so double-clicked GUI deployments do not open a console window. Debug builds keep the console for local diagnostics.

## Practical Paths

- Easiest shared-code setup remains WSL2 when Linux-equivalent terminal persistence is required.
- Native Windows now has a basic path, but terminal sessions do not survive a `webclx` process restart.
- Do not share build artifacts between machines. Keep `target/` ignored and preferably set per-machine `CARGO_TARGET_DIR` outside the synced source tree.
- Keep runtime config/session/preset JSON files machine-local when paths differ; shared defaults should live in templates or documented examples, not in the active per-machine files.
- Native Windows deployments may use workspace and favorite paths on non-profile drives such as `D:\UserData\...`; validation should allow existing absolute Windows paths instead of forcing them under the current user's home.
- If a saved Linux terminal user such as `root` is invalid on Windows, fall back to the current Windows user and keep the rest of the settings file instead of discarding the whole file.
- When replacing the local Windows exe, remember `webClx` serves disk `static/` before embedded assets. Sync the deployment `static/` directory with the built source unless intentionally testing embedded fallbacks.

## Windows Deployment Contract

Routine deployment to the LAN Windows client uses the project skill and its deterministic script:

```bash
bash .codex/skills/webclx-windows-deploy/scripts/deploy-webclx-windows.sh
```

The script deliberately keeps these constraints together:

- Cross-build the current worktree for `x86_64-pc-windows-gnu`. This target remains an explicit exception to the Linux webClx self-deploy compile queue.
- Call `POST http://192.168.3.38:15301/api/processes/replace-restart` with `exe=webclx.exe`, `arg=serve`, and the built file. Omit `target_path`; rustCommander resolves the path from the running process and returns the runtime directory.
- Keep `arg=serve`. The deployed file is lowercase `webclx.exe`, while the legacy no-argument path in `src/cli.rs` recognizes the exact mixed-case basename `webClx`; omitting the argument can make replacement report `restarted=true` even though the child prints help and exits.
- Synchronize all of `static/` using the runtime directory returned by rustCommander, with a staged extraction and the single rollback slot `static.bak-prev`.
- Verify the fresh process, `webclx version`, local/remote exe SHA-256, `/api/auth/session` HTTP 200, and the SHA-256 of HTTP-served `/assets/app.js`. The replace response alone is not a health check.

If the same-name process is already stopped, path-free replacement cannot infer its target. Treat that as a recovery case and report it; do not turn routine deployment back into target-path discovery.
