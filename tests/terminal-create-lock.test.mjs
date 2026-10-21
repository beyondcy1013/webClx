import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..");
const source = fs.readFileSync(path.join(repoRoot, "src/terminal/manager.rs"), "utf8");

test("explicit terminal creation does not hold the manager write lock during backend startup", () => {
  const start = source.indexOf("fn create_session_inner(");
  const end = source.indexOf("\n    fn with_local_api_token_file(", start);
  assert.notEqual(start, -1, "create_session_inner should remain the explicit-create seam");
  assert.notEqual(end, -1, "create_session_inner should have a bounded source body");

  const body = source.slice(start, end);
  assert.doesNotMatch(
    body,
    /create_session_locked\(/,
    "explicit creation must reserve metadata separately from the backend startup path",
  );
});
