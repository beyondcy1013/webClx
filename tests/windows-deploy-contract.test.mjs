import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const script = readFileSync(
  new URL(
    "../.codex/skills/webclx-windows-deploy/scripts/deploy-webclx-windows.sh",
    import.meta.url,
  ),
  "utf8",
);

test("Windows deploy uses process auto-location and an explicit serve argument", () => {
  const replaceRequest = script.match(
    /replace_response=\$\(curl[\s\S]+?\n    -F \"file=@\$ARTIFACT\"\)/,
  )?.[0];

  assert.ok(replaceRequest, "replace-restart request should be present");
  assert.match(replaceRequest, /api\/processes\/replace-restart/);
  assert.match(replaceRequest, /-F 'arg=serve'/);
  assert.doesNotMatch(replaceRequest, /target_path=/);
  assert.match(script, /target_path=OMITTED/);
});

test("Windows deploy synchronizes and verifies disk static assets", () => {
  assert.match(script, /PROJECT_DIR\/static/);
  assert.match(script, /static\.bak-prev/);
  assert.match(script, /WEBCLX_URL\/assets\/app\.js/);
  assert.match(script, /remote_app_hash/);
  assert.match(script, /WEBCLX_URL\/api\/auth\/session/);
});

test("Windows deploy expands the exe path for version and hash verification", () => {
  assert.doesNotMatch(script, /runtime_dir\\\$EXE_NAME/);
  assert.match(script, /\$\{runtime_dir\}\\+\$\{EXE_NAME\}/);
});
