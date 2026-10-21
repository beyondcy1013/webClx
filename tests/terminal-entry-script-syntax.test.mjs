import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

const staticRoot = new URL("../static/", import.meta.url);
const terminalHtml = readFileSync(new URL("terminal.html", staticRoot), "utf8");

test("terminal page scripts are valid JavaScript", () => {
  const scriptPaths = Array.from(
    terminalHtml.matchAll(/<script\s+defer\s+src="\/assets\/([^"?]+)(?:\?[^"?]*)?"\s*><\/script>/g),
    (match) => match[1],
  );

  assert.ok(scriptPaths.includes("terminal.js"), "terminal page should load terminal.js");

  for (const scriptPath of scriptPaths) {
    const source = readFileSync(new URL(scriptPath, staticRoot), "utf8");
    assert.doesNotThrow(
      () => new vm.Script(source, { filename: scriptPath }),
      `${scriptPath} should parse before it is shipped to browsers`,
    );
  }
});
