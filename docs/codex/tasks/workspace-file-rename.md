# Workspace File Rename

工作区文件浏览页支持直接重命名普通文件和文件夹。

Stable conclusions:

- Backend endpoint: `POST /api/file/rename` with `{ "path": "<workspace-relative path>", "name": "<new basename>" }`.
- Rename is same-directory only: `name` is a basename and must not contain `/` or `\`.
- Backend only renames ordinary files and directories. Symlinks and other special entries are rejected.
- Target names must not already exist; conflict checks use `symlink_metadata` so broken symlinks still block overwrite.
- Frontend rows render `改名` in `static/app.js`; after rename it refreshes the directory, updates the current editor path/title, and migrates matching favorite paths.
- Frontend changes require syncing `/home/bin/webclx/static/`; backend route changes require rebuilding and reinstalling `/home/bin/webclx/webClx`.

Common files:

- `src/filesystem.rs`
- `src/main.rs`
- `static/app.js`
- `static/index.html`
- `crates/api_catalog_core/src/bodies/files.rs`
- `crates/api_catalog_core/src/endpoints.rs`
- `crates/api_catalog_core/src/notes.rs`
