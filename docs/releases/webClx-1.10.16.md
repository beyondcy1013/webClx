# webClx 1.10.16

webClx 1.10.16 is the third GitHub public-source preview. It adds prebuilt
Linux (x86_64 and arm64), Windows, and Android artifacts next to the sanitized
source snapshot.

## Changes

- Devin accounts: separate CLI and Desktop account types with isolated
  snapshots, import-time new/old judgement with duplicate-id convergence, and
  encrypted Desktop login snapshot tooling.
- CodeBuddy accounts: import accepts nested `auth`/`auth_raw` and
  `account`/`profile_raw` payloads, keeps existing refresh/access tokens when
  an incoming record leaves them empty, and clipboard export carries full
  credentials again.
- Account interchange: the shared import/export dialog is wired into the
  account clipboard manager, so `index.html` no longer references missing
  assets.
- Terminal runtime: backspace repeat, first-scroll rebound after page restore,
  compressed-state detection based on real compaction output, Qoder agent
  recognition, and Codex narration-style busy line handling.
- Compile queue: build duration and live log summary are shown in the request
  list.
- Windows: PTY child errno comparisons use `cfg(unix)` constants, so the
  `x86_64-pc-windows-gnu` release build links without `libc`.

## Prebuilt artifacts

| Platform | File |
| --- | --- |
| Linux x86_64 (glibc) | `webClx-1.10.16-linux-x86_64.tar.gz` |
| Linux arm64 (static musl) | `webClx-1.10.16-linux-arm64.tar.gz` |
| Windows x86_64 | `webClx-1.10.16-windows-x86_64.zip` |
| Android (arm64/arm/x86_64 WebView) | `webClx-1.10.16-android.apk` |
| Source | `webClx-1.10.16-source.tar.gz` |

Every artifact ships with a `.sha256` checksum. The Linux archives contain the
`webclx` binary plus the runtime `static/` directory; the Windows archive
contains `webclx.exe` and `static/`.

## Running the binaries

```bash
# Linux x86_64 / arm64
tar -xzf webClx-1.10.16-linux-x86_64.tar.gz   # or -linux-arm64
cd webClx-1.10.16-linux-x86_64 && ./webclx serve
```

```powershell
# Windows
Expand-Archive webClx-1.10.16-windows-x86_64.zip -DestinationPath .
cd webClx-1.10.16-windows-x86_64; .\webclx.exe serve
```

Keep the bundled `static/` directory next to the executable; the runtime serves
it from disk and falls back to the embedded copy.

## Build notes

All artifacts are built from this sanitized source tree. The arm64 build uses
`aarch64-unknown-linux-musl` with `zig cc` as the cross linker
(`-C link-self-contained=no`), so the binary is static and needs no system
libc. The Android APK contains no native libraries; the WebView UI is shared
with the desktop build.

## Android

The public APK is built from this sanitized source, so the compiled-in source
list carries `HOST:PORT` and `PUBLIC_DOMAIN:PORT` placeholders instead of any
real deployment. Open your own server address in the app (link or intent
handled by `openInWebView`) to point it at your instance, or edit
`android/app/src/main/java/com/webclx/app/SourceRegistry.java` and rebuild with
`scripts/build-android-apk.sh`.

## Source archive

- Source commit: `dc23bbe599ec`
- License: GNU AGPL-3.0-or-later

The archive includes `SOURCE_RELEASE` and a `STATIC_ASSETS_MANIFEST.sha256`
covering every `static/` file.

## Scope

This is a developer preview. webClx can edit files and execute terminal
commands, so do not expose its management port directly to the internet. Put
remote deployments behind TLS and network controls.
