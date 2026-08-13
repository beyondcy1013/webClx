# Security Policy

## Supported Version

Only the latest published webClx release receives security fixes during the developer preview.

## Deployment Boundary

webClx is an administrative tool. It can access workspaces, terminal processes, model credentials, and deployment actions.

- Do not expose port `11111` directly to the public Internet.
- On first start, retrieve the random initial credential from `.webclx-initial-password` in the runtime working directory through the host console. New installations use `webclx` as the default username; upgrades preserve the existing username. The file is owner-readable only and is removed after the first successful login.
- Use TLS, a reverse proxy, firewall rules, and a private network or VPN.
- Loopback alone is not trusted. Local automation bypasses browser login only when it also presents the server-generated token from `.webclx-local-api-token`; both files must remain owner-readable only.
- Run webClx under a dedicated operating-system user where possible and scope its workspace root narrowly.
- Never send passwords, cookies, API keys, or access tokens through terminal messages.

## Reporting a Vulnerability

Do not open a public issue containing an exploit, credential, or sensitive log. Contact the repository owner privately through the security contact published with the release. Include the affected version, reproduction steps, impact, and a minimal proof of concept.

We will acknowledge a complete report, investigate it, and coordinate disclosure after a fix is available. This file does not create a bug-bounty program or promise a specific response time.
