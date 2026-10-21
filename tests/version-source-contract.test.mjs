import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const main = readFileSync(new URL("../src/main.rs", import.meta.url), "utf8");
const system = readFileSync(new URL("../src/system.rs", import.meta.url), "utf8");

test("runtime and system APIs use the Cargo package version", () => {
  assert.match(
    main,
    /version:\s*env!\("CARGO_PKG_VERSION"\)\.to_string\(\)/,
  );
  assert.match(
    system,
    /const APP_VERSION:\s*&str\s*=\s*env!\("CARGO_PKG_VERSION"\)/,
  );
  assert.doesNotMatch(main, /version:\s*"\d+\.\d+\.\d+"\.to_string\(\)/);
  assert.doesNotMatch(system, /const APP_VERSION:\s*&str\s*=\s*"\d+\.\d+\.\d+"/);
});
