# webClx 1.8.11

webClx 1.8.11 is the first GitHub public-source preview. It keeps the 1.8.10
runtime behavior while tightening source-release boundaries before publication.

## Changes

- Exclude local Agent configuration, internal cross-model execution records,
  terminal history, runtime credentials, and host-specific remote deployment
  tooling from source archives.
- Keep the portable terminal messaging, artifact publishing, and compile queue
  Skills needed to use and verify a source checkout.
- Replace a credential-shaped production-domain test fixture with reserved
  `example.test` data.
- Generate portable checksum files that work after downloading into any local
  directory.
- Add release tests that prevent internal execution records and credentialed
  `fpsq.xyz` URLs from entering future source packages.

## Source archive

- File: `webClx-1.8.11-source.tar.gz`
- SHA-256: `a020193f1145b9a01a3a9726c3ee8b467c69e5164b85b01c0f22c07971f7d35f`
- Source commit: `eae65b782b67`
- License: GNU AGPL-3.0-or-later

The archive includes `SOURCE_RELEASE` and a 111-file
`STATIC_ASSETS_MANIFEST.sha256`. Verify the attached checksum before building.

## Scope

This is a developer preview. webClx can edit files and execute terminal
commands, so do not expose its management port directly to the internet. Put
remote deployments behind TLS and network controls. Unrelated hosted-trial
customers must never share one administrative instance.
