---
name: webclx-artifact-publisher
description: Use when publishing verified versioned APK, Windows, archive, or browser artifacts via webClx with latest-three retention.
---

# webClx Artifact Publisher

Use this skill after a build creates an artifact that the user should download from a browser. Publishing is data-driven: never edit `/downloads`, `index.json`, SQL, or embedded HTML for a release. Put the file into the managed drop directory or use the helper; webClx discovers it and renders the page dynamically.

Current default policy:

- Publish Android APKs after successful Android builds. APK publication must update both the download catalog and the Android update manifest consumed by clients.
- Give every downloadable release artifact a versioned filename. APK names must use a form such as `lyyNote-0.1.8.apk`; never publish a generic APK name such as `lyyNote.apk`.
- Retain at most the latest three artifacts for each project. A successful publish must leave the new artifact plus at most two older artifacts, ordered by `published_at`; webClx removes older records and their managed files.
- Publish Windows build outputs when the user needs to install or download them on another machine.
- Do not publish Linux build outputs that are meant to run or deploy directly on the same machine.
- Publish Linux artifacts only when the user explicitly asks for a download link or remote transfer artifact.

## Quick Start

Preferred for releases, metadata, and every APK:

```bash
bash /home/root/.codex/skills/webclx-artifact-publisher/scripts/publish-artifact.sh \
  --project newsKB \
  --path /home/codes/newsKB/android/app/build/outputs/apk/published/newsKB_writing.apk \
  --version 0.4.2 \
  --name newsKB-writing-0.4.2.apk \
  --label "newsKB 写作 Android 0.4.2" \
  --note "Android release build"
```

Default webClx base URL is `http://127.0.0.1:11111`. Override with `--base-url` or `WEBCLX_BASE_URL`.

For a metadata-free artifact on the webClx host, direct drop is also supported:

```bash
project="myProject"
name="myProject-1.2.3-windows-x64.zip"
drop_dir="/home/bin/webclx/.webclx-artifacts/$project"
install -d "$drop_dir"
cp "/absolute/path/to/$name" "$drop_dir/$name.part"
mv "$drop_dir/$name.part" "$drop_dir/$name"
curl -fsS --noproxy '*' http://127.0.0.1:11111/api/artifacts >/dev/null
```

Use a `.part` or `.tmp` suffix while copying, then rename atomically. webClx ignores incomplete files, discovers the final name, computes its hash, groups it by the parent project directory, and retains the latest three project artifacts.

## Workflow

1. Build and verify the artifact with the project-specific process first.
2. Read the authoritative application version from the built artifact or project metadata. Do not infer or invent a version from timestamps or previous releases.
3. Decide whether the artifact needs a browser download URL. Android and Windows outputs usually do; same-machine Linux outputs usually do not.
4. Choose one publication path:
   - Use `publish-artifact.sh` for APKs, release notes, custom labels, or any artifact that needs strict response verification.
   - Use direct drop only when the file name itself is sufficient metadata.
5. Use the script with the final artifact path, not an intermediate temporary file. For APKs, pass the verified version through `--version`. The script appends it to a generic source filename automatically; pass `--name` when a clearer product filename is needed.
6. For an APK, the helper must query `/api/artifacts/update/android/{project}` after publication and fail unless its version, file name, SHA-256, size, and download URL match the newly published artifact. This manifest is the update center; it is derived from the artifact index rather than a separate SQL write.
7. Read the JSON response and query `/api/artifacts`. Verify that the matching project contains the newly published artifact and no more than three entries.
8. Report:
   - `download_url` or absolute download URL
   - `/downloads` page URL
   - `/api/artifacts/update/android/{project}` URL for APKs
   - project, file name, size
9. If webClx is unavailable, say so and keep the local artifact path visible.

## Migrating Old Publishers

- Replace scripts that edit a downloads HTML page, JSON index, or SQL row with one call to `publish-artifact.sh`.
- Replace plain `cp` commands that expose a partially copied final name with `.part` copy plus atomic rename.
- Keep project-specific build and signing logic in the project. Centralize only artifact registration and verification in this skill.
- Do not maintain per-project download-page markup. The parent directory is the project/software group; versioned files become separate rows under that group automatically.
- Do not delete older versions manually. The server owns latest-three retention.

## Script Options

```text
--project <name>   Project group shown on the download page. Defaults to cwd basename.
--path <file>      Required absolute artifact path.
--version <value>  Required for APKs. Must match the verified application version.
--name <name>      Download filename. Defaults to source file basename.
--label <label>    Human label shown on the page. Defaults to filename.
--note <note>      Optional build note.
--base-url <url>   webClx origin. Defaults to WEBCLX_BASE_URL or http://127.0.0.1:11111.
```

For APKs, the script rejects a missing `--version`, a non-APK download suffix, or a download name that omits the supplied version. If the source APK is named `lyyNote.apk`, `--version 0.1.8` produces `lyyNote-0.1.8.apk`; an explicit `--name` must also contain `0.1.8`.

The webClx API stores files under `.webclx-artifacts/` in the running webClx app directory and serves them through stable HTTP download endpoints. The Android update center selects its latest eligible version from that same index; there is no separate SQL table to update. Rely on the server retention policy; do not manually edit `index.json` to prune versions. Do not publish secrets, signing keys, config files with credentials, or unverified build outputs.
