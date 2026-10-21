# Artifact Download Center

Use the unified download center when a build output needs to be published through webClx.

## Verified Pattern

- `POST /api/artifacts/publish` copies an absolute local artifact into `.webclx-artifacts/<project>/`.
- Files copied directly to `.webclx-artifacts/<project>/<file-name>` are discovered automatically when the catalog or Android update manifest is requested. Discovery computes SHA-256, moves the file into managed storage, updates `index.json`, and applies latest-three retention.
- Files ending in `.tmp` or `.part`, and dotfiles, are ignored so incomplete transfers are not published. Copy with a temporary suffix and rename to the final file name when the transfer completes.
- `GET /downloads` renders all published projects and artifacts in one place.
- `GET /api/artifacts` returns the grouped JSON catalog.
- `GET /api/artifacts` orders projects by each project's newest artifact publish time, descending; artifacts inside each project are also newest first.
- `GET /api/artifacts/download/{artifact_id}/{file_name}` serves the stored file with download headers.
- The terminal `全能` menu includes `下载中心`, which opens `/downloads` in a new tab.
- `/downloads` is a webClx child page with the same top-level navigation and theme preference as the main interface.
- `/downloads` formats `published_at` RFC3339 values as browser-local `YYYY-MM-DD HH:mm:ss` text instead of exposing the raw ISO timestamp.
- `/downloads` defaults to a global newest-first publish-time sort. Every column header is clickable and toggles ascending/descending order; each artifact keeps its project value in its own row so cross-project sorting remains accurate. The page also provides an `立即刷新` button that rescans the artifact directory.

## Skill

- `webclx-artifact-publisher` publishes any local build output to the running webClx service.

## Notes

- Keep published artifacts separate from the source tree.
- Use the webClx app directory for storage so restart/rebuild can rediscover the catalog.
- The deployed service normally uses `/home/bin/webclx/.webclx-artifacts/<project>/` as the direct-drop root.
