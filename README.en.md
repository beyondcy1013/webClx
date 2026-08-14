# webClx

[中文](README.md) | English

[Download v1.8.11](https://github.com/beyondcy1013/webClx/releases/tag/v1.8.11) ·
[Request a 7-day isolated hosted trial](https://github.com/beyondcy1013/webClx/issues/new?template=hosted-trial.yml) ·
[Product page](https://beyondcy1013.github.io/webClx/) ·
[Commercial support and licensing](COMMERCIAL.md) ·
[Discussions](https://github.com/beyondcy1013/webClx/discussions)

![Synthetic webClx demonstration showing a desktop coding terminal continued from a phone](site/assets/webclx-remote-workflow.png)

The demonstration above is synthetic and contains no customer data, credentials, or internal project paths.

webClx is a self-hosted workspace control plane for persistent coding terminals and AI coding agents. It keeps Codex, Claude, DeepSeek Harness, and ordinary shells visible in one browser workspace while preserving each tool's native terminal and context.

## Why webClx

- Persistent browser terminals backed by tmux, including mobile-friendly controls.
- Workspace browsing and UTF-8 editing from the same UI.
- Codex and Claude provider presets, protocol adapters, and per-launch isolation.
- Reliable terminal-to-terminal task handoff with delivery confirmation and reply routing.
- A built-in Agent, Skill discovery, build/deploy queues, and downloadable artifacts.
- Chinese and English UI with a per-browser language preference.
- A bundled `webclx-terminal-message` Skill installed into the Codex, Claude, and DeepSeek Harness user Skill roots (`~/.codex/skills`, `~/.claude/skills`, and `~/.dsh/skills` by default).

webClx complements Agent Harnesses; it does not replace them. The current release is a developer preview intended for trusted self-hosted environments.

The mobile advantage is access to the complete development environment: monitor builds and deployments, inspect logs, and resume long-running Agent work away from a desk. It is not a reduced IDE recreated for a phone.

## Quick Start

Requirements: Rust stable with edition 2024 support, `tmux`, and Node.js for frontend tests.

```bash
git clone https://github.com/beyondcy1013/webClx.git
cd webClx
cargo run --release -- serve
```

Versioned source archives and checksum files are available from [GitHub Releases](https://github.com/beyondcy1013/webClx/releases). Each preview archive contains `SOURCE_RELEASE` and `STATIC_ASSETS_MANIFEST.sha256` provenance files.

Open `http://127.0.0.1:11111`. On first start, webClx generates a random password and writes the one-time recovery credential to `.webclx-initial-password` in the runtime working directory with owner-only permissions. New installations use `webclx` as the default username; upgrades preserve the existing username. Read the recovery file from the host console and store the credential in a password manager; the file is removed after the first successful login. Put a TLS reverse proxy and network access controls in front of any non-local deployment.

Prefer a managed setup? [Request a 7-day isolated hosted trial](https://github.com/beyondcy1013/webClx/issues/new?template=hosted-trial.yml). Trial instances do not share administrator accounts, cookies, workspaces, or model credentials with maintainers or other customers.

Run the baseline checks:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
node --test tests/*.test.mjs
```

## Agent Handoff

The bundled Skill uses `POST /api/terminal/sessions/message` and verifies that a Codex or Claude prompt entered the destination rollout. A typical read-only review handoff is:

```bash
python3 ~/.codex/skills/webclx-terminal-message/scripts/send_terminal_message.py \
  --target project-review \
  --from project-implement \
  --message 'Review the current diff read-only and report concrete findings.' \
  --request-reply
```

Use one writer per working tree. Other Agents should review read-only unless their workspaces are deliberately isolated.

## Security Scope

webClx can edit files, run terminals, manage model credentials, and trigger deployment. Treat it as administrative infrastructure. Loopback API automation must present the server-generated token from `.webclx-local-api-token`; remote requests require a signed session cookie. See [SECURITY.md](SECURITY.md) before exposing it outside a trusted network.

## License and Commercial Use

webClx is licensed under [GNU AGPL-3.0-or-later](LICENSE). Organizations that cannot comply with AGPL network-source obligations may request a separate commercial license and support agreement.

See [COMMERCIAL.md](COMMERCIAL.md) for hosted plans, deployment support, licensing, and trial boundaries.

See the [trial and commercialization playbook](docs/trial-commercial-playbook.md) and the [Chinese/English launch copy](docs/launch-copy.md) for the invite-only hosted preview, isolation boundaries, suggested pricing, and channel plan.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution requirements.
