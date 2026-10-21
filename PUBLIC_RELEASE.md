# Public release provenance

This repository continues from the audited webClx 1.8.11 public snapshot. The
application source now corresponds to `SOURCE_RELEASE` commit `ada451c962ed`
(version 1.10.22). Public Git commits overlay sanitized source snapshots and
add public release metadata; the private development history is never merged.

The versioned archive, its SHA-256 checksum, and the prebuilt Linux (x86_64 and
arm64), Windows, and Android artifacts are attached to the GitHub `v1.10.22`
release. Verify the archive before building it.

Internal agent configuration, cross-model execution records, runtime
credentials, local settings, terminal history, remote deployment scripts,
private infrastructure endpoints, build output, and the private development
Git history are intentionally excluded from the public source release. Agent
skills (`.codex/skills` and every `SKILL.md`) are never published.
