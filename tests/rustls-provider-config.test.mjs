import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const manifest = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8");
const tokioRustls = manifest.match(/^tokio-rustls\s*=\s*\{[^\n]+\}$/m)?.[0] ?? "";

test("rustls dependencies use one crypto provider", () => {
  assert.match(tokioRustls, /features\s*=\s*\[[^\]]*"aws_lc_rs"/);
  assert.doesNotMatch(tokioRustls, /features\s*=\s*\[[^\]]*"ring"/);
});
