# Remote Skill Sync

Use `sync-server-xiaoshuai` to deploy selected webClx artifacts to the remote server.

## Verified Pattern

- Directory sources must be passed as `path/` so the directory contents land in the intended remote directory.
- File sources can be pushed directly.
- For webClx, do not mirror the repository root to xiaoshuai. Use:
  `sync-server-xiaoshuai deploy-webclx /home/codes/webClx /home/bin/webclx /home/codes/webClx`
- The deploy command copies only the release binary and `static/`, and prunes remote Rust source/project files while preserving runtime JSON config.
- The xiaoshuai `webclx.service` should use `/home/bin/webclx` as `WorkingDirectory` and `/home/bin/webclx/webclx` as `ExecStart`; otherwise static files and runtime JSON may still be read from a stale source checkout.
