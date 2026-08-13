# Contributing to webClx

Thank you for helping improve webClx.

## Before Opening a Change

- Discuss large behavior or compatibility changes first.
- Keep one concern per pull request.
- Do not include credentials, session data, model transcripts, build logs, or machine-specific runtime state.
- Preserve the existing Codex/Claude configuration and terminal ownership boundaries documented in `AGENTS.md` and `docs/codex/`.

## Development

Use Rust stable with edition 2024 support, Node.js, and tmux. Run focused tests while developing and the full applicable baseline before submission:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
node --test tests/*.test.mjs
```

Add a regression test for every bug fix. User-facing browser changes must remain keyboard accessible, work at mobile widths, and include Chinese and English copy in the shared localization runtime.

Use Conventional Commits. By submitting a contribution, you certify that you have the right to provide it under the project's AGPL-3.0-or-later license.
