# webClx 1.10.15

webClx 1.10.15 is the second GitHub public-source preview. It carries the
runtime forward from the 1.8.11 snapshot while keeping the same public source
boundary.

## Changes

- Compile queue and compile API: request status queries with progress and log
  tails, configurable global timeouts with abort-and-alert behavior, compile
  environment variable panel, build/run record retention, and a shared Cargo
  target cache layout.
- Terminal workspace: persisted terminal scenes with restore, scheduled and
  automatic-continue message delivery, server-side delivery ledger,
  multi-terminal discussion grouping, inline rename, and mobile soft-keyboard
  layout fixes.
- Agent presets and models: Codex/Claude/DeepSeek preset switching with
  shared-config apply and restore, provider quotas and package queries,
  multi-account OAuth/API-key import, and a native built-in Agent surface.
- Web UI: LLM_API account managers, workspace browser, document tools, FRP and
  sub2api manager views, login session lifetime settings, and navigation
  cleanup.
- Android client: concurrent fastest-source selection, public update download
  route, WebView lifecycle and keyboard reliability fixes.
- Source release: `builtin-skills/` ships as an empty placeholder so the
  compile-time skill bundle builds from a clean checkout; internal Skills,
  Agent configuration, execution records, and deployment tooling remain
  excluded.
- Sanitization: hosted-domain, LAN, FRP-tunnel, and provider endpoints in
  docs, fixtures, and UI option lists are replaced with `PUBLIC_DOMAIN`,
  `HOST`, `PORT`, `proxy.example.com:PORT`, or `example.invalid` placeholders;
  credential-shaped fixtures were replaced with reserved example data.

## Source archive

- File: `webClx-1.10.15-source.tar.gz`
- Source commit: `1b4114873178`
- License: GNU AGPL-3.0-or-later

The archive includes `SOURCE_RELEASE` and a `STATIC_ASSETS_MANIFEST.sha256`
covering every `static/` file. Verify the attached checksum before building.

## Scope

This is a developer preview. webClx can edit files and execute terminal
commands, so do not expose its management port directly to the internet. Put
remote deployments behind TLS and network controls. Unrelated hosted-trial
customers must never share one administrative instance.
