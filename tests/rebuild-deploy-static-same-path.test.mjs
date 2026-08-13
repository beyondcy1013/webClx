import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const script = readFileSync(
  new URL("../scripts/rebuild-and-deploy.sh", import.meta.url),
  "utf8",
);

test("same static directory skips synchronization but still reaches restart", () => {
  const branchStart = script.indexOf(
    'if [ "$SOURCE_REAL" = "$TARGET_REAL" ]; then',
  );
  const restart = script.indexOf('log "restarting webClx service"', branchStart);
  assert.ok(branchStart >= 0, "same-directory branch must exist");
  assert.ok(restart > branchStart, "restart must remain after static handling");

  const staticHandling = script.slice(branchStart, restart);
  const elseStart = staticHandling.indexOf("\nelse\n");
  assert.ok(elseStart > 0, "static synchronization must be in the else branch");

  const sameDirectoryBranch = staticHandling.slice(0, elseStart);
  const differentDirectoryBranch = staticHandling.slice(elseStart);
  assert.doesNotMatch(sameDirectoryBranch, /\brsync\b|\bcp -r\b|\bfind\b/);
  assert.match(differentDirectoryBranch, /\brsync\b/);
  assert.match(differentDirectoryBranch, /\bcp -r\b/);
  assert.doesNotMatch(sameDirectoryBranch, /\bexit\s+0\b/);
});
