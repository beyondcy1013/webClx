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

## Host log limits

Install the logrotate and journald snippets only after reviewing the host's
existing logging services. The included systemd timer runs the dedicated
logrotate policy hourly so high-volume logs cannot grow unchecked between
daily system jobs.

The US trial candidate was prepared with these limits on 2026-08-14. Its npm
content-addressed download cache was also cleared because it occupied about
21 GB and contained no project or configuration data. Application data and
global npm installations were not removed.
