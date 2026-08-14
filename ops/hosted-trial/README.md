# Hosted trial host preparation

These files prepare a host for isolated, invite-only webClx trials. They do
not open a public trial service by themselves.

## Instance plan

Generate a secret-free plan and reviewable configuration bundle:

```bash
scripts/hosted-trial-instance.sh \
  --customer-id demo-01 \
  --port 12101 \
  --tls-cert /etc/letsencrypt/live/trial-demo-01.fpsq.xyz/fullchain.pem \
  --tls-key /etc/letsencrypt/live/trial-demo-01.fpsq.xyz/privkey.pem \
  --render-dir /tmp/webclx-trial-demo-01
```

The generated service uses a dedicated OS user and directories, systemd
sandboxing, memory/CPU/task limits, a loopback nginx upstream, WebSocket
headers, and an iptables rule that rejects non-loopback access to the source
port.

`--apply` is intentionally fail-closed. It requires an exact confirmation,
verified binary and static assets, readable customer-specific TLS files,
working DNS, a free source port, and the configured minimum free disk space.
The current implementation stops after readiness checks; it never creates a
customer account or partially installs an instance.

Do not reduce these gates to make a trial appear available. Provisioning must
remain blocked until customer-specific DNS and TLS are externally verifiable.

## Non-public QA lifecycle

Use `scripts/hosted-trial-qa-lifecycle.sh` only for disposable instances whose
customer ID begins with `qa-`. It defaults to a secret-free dry-run; real
changes require `--apply`, and deletion requires an exact `--confirm-delete`.

The lifecycle creates a dedicated OS user, app/workspace/artifact directories,
a resource-limited systemd unit, and a commented iptables rejection rule before
starting the service. webClx 1.8.11 normalizes the configured loopback bind to
`0.0.0.0`, so the firewall rule is an active security boundary and must be
verified from a network path that does not use an HTTP proxy.

`freeze` stops the service and removes workspace write permission. `export`
refuses symbolic links and archives only the workspace. `delete` removes the
service, firewall rule, OS user, runtime directories, and (unless explicitly
kept) exports, then checks for residue.

On 2026-08-14 the complete disposable lifecycle was verified on the US trial
candidate as `qa-us-01` on port `12101`: loopback HTTP worked, direct public
access failed, systemd limits were active, a synthetic workspace was frozen
and exported, and final deletion left no unit, user, port, firewall rule,
runtime directory, or export. The existing `webclx.service` remained active.

### Capacity and encrypted backups

Use `scripts/hosted-trial-data-guard.sh` for disposable `qa-` instances. The
tool reads the lifecycle manifest without sourcing it, reports workspace and
artifact usage separately, and stops the service before making both data
directories read-only when a byte limit is exceeded. This is an application
capacity guard, not a native filesystem quota: the current US XFS root is
mounted without quota support.

`backup` rejects symbolic links and encrypts only `workspace/` to an explicit
customer GPG fingerprint. It writes a mode-0600 `.tar.gz.gpg` file and SHA-256
sidecar through temporary files. `restore` verifies the checksum, decrypts to
a private temporary directory, rejects paths outside `workspace/`, links, and
special files, and extracts only into an explicit empty restore directory. A
restore never targets the live instance directory.

On 2026-08-14 this data guard was exercised on the US trial candidate with a
new disposable `qa-us-01` instance and one-time GPG key. The encrypted file did
not expose the synthetic workspace plaintext, the restored workspace matched
byte-for-byte and contained no artifacts, a zero-MiB limit stopped the QA
service and removed all write bits, and cleanup left no QA user, unit, port,
firewall rule, runtime data, backup, restore tree, or temporary key. The main
`webclx.service` remained active.

### Scheduled maintenance

`scripts/hosted-trial-maintenance.sh` reads one mode-0600 or mode-0400 config
per QA customer from `/etc/webclx/trials`. It never sources config as shell
code. Unknown fields, duplicate fields, filename/customer mismatches, symbolic
links, non-exact GPG fingerprints, and invalid limits fail closed. Start from
`customer-maintenance.conf.example`, replace every placeholder, and keep the
customer keyring outside home directories so the hardened systemd service can
read it.

The hourly timer always checks capacity. After the configured UTC backup hour,
it creates at most one encrypted backup per UTC date and keeps only the newest
configured number of complete ciphertext/checksum pairs. Unrelated or
incomplete files are never removed by retention cleanup.

The maintenance scripts and units were installed on the US candidate on
2026-08-14. The timer remains disabled because there is no real customer key
configuration. A disposable instance was run through the installed scheduler
twice; capacity was checked twice, exactly one encrypted backup was produced,
and final cleanup removed the instance, key, backup, state, and config while
the main service stayed active. Enable the timer only after reviewing a real
customer config and completing that customer's first restore test:

```bash
systemctl enable --now webclx-trial-maintenance.timer
systemctl list-timers webclx-trial-maintenance.timer
```

This evidence does not open hosted trials. Customer DNS/TLS, public login
flows, a reviewed real-customer key/config and enabled schedule, native hard
quotas or equivalent continuous containment, and business/legal details remain
separate release gates.

## Host log limits

Install the logrotate and journald snippets only after reviewing the host's
existing logging services. The included systemd timer runs the dedicated
logrotate policy hourly so high-volume logs cannot grow unchecked between
daily system jobs.

The US trial candidate was prepared with these limits on 2026-08-14. Its npm
content-addressed download cache was also cleared because it occupied about
21 GB and contained no project or configuration data. Application data and
global npm installations were not removed.
