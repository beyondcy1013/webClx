# webClx 1.10.22

webClx 1.10.22 is the fifth GitHub public-source preview. It removes agent
skills from the published source, sanitizes the remaining FRP test fixture, and
ships prebuilt Linux (x86_64 and arm64), Windows, and Android artifacts.

## Changes

- Skills are no longer published: `.codex/skills` and every `SKILL.md` are
  stripped from the source archive, together with the tests that exercised the
  removed skill scripts.
- FRP test fixture now uses example values (`frp.example.com`, port `7000`,
  `example-token`, `192.168.1.10`) instead of a real control endpoint.
- Version: package version raised to 1.10.22; no runtime behaviour change over
  1.10.21.
- Runtime carried forward from 1.10.16/1.10.21:
  - Devin accounts: separate CLI and Desktop account types with isolated
    snapshots and encrypted Desktop login snapshot tooling.
  - CodeBuddy accounts: nested `auth`/`auth_raw` and `account`/`profile_raw`
    import, token-preserving upsert, and full-credential clipboard export.
  - Account interchange: shared import/export dialog wired into the account
    clipboard manager.
  - Terminal runtime: backspace repeat, first-scroll rebound, compaction-state
    detection, Qoder agent recognition, and Codex narration busy lines.
  - Compile queue: build duration and live log summary in the request list.
  - Windows: PTY child errno comparisons use `cfg(unix)` constants.

## Prebuilt artifacts

| Platform | File |
| --- | --- |
| Linux x86_64 (glibc) | `webClx-1.10.22-linux-x86_64.tar.gz` |
| Linux arm64 (static musl) | `webClx-1.10.22-linux-arm64.tar.gz` |
| Windows x86_64 | `webClx-1.10.22-windows-x86_64.zip` |
| Android | `webClx-1.10.22-android.apk` |
| Source | `webClx-1.10.22-source.tar.gz` |

Every artifact ships with a `.sha256` checksum.

## Running the binaries

```bash
tar -xzf webClx-1.10.22-linux-x86_64.tar.gz   # or -linux-arm64
cd webClx-1.10.22-linux-x86_64 && ./webclx serve
```

```powershell
Expand-Archive webClx-1.10.22-windows-x86_64.zip -DestinationPath .
cd webClx-1.10.22-windows-x86_64; .\webclx.exe serve
```

Keep the bundled `static/` directory next to the executable.

## Android

The public APK is built from this sanitized source; the compiled-in source list
carries `HOST:PORT` / `PUBLIC_DOMAIN:PORT` placeholders. Open your own server
address in the app (link or intent handled by `openInWebView`), or edit
`android/app/src/main/java/com/webclx/app/SourceRegistry.java` and rebuild with
`scripts/build-android-apk.sh`.

## Source archive

- Source commit: `ada451c962ed`
- License: GNU AGPL-3.0-or-later

The archive includes `SOURCE_RELEASE` and a `STATIC_ASSETS_MANIFEST.sha256`
covering every `static/` file. `builtin-skills/` ships as an empty placeholder.

## Scope

This is a developer preview. webClx can edit files and execute terminal
commands, so do not expose its management port directly to the internet.
