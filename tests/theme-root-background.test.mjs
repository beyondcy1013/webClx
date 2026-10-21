import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const styles = readFileSync(
  new URL("../static/styles-base.css", import.meta.url),
  "utf8",
);

test("the document root keeps an opaque theme background during tab reflow", () => {
  assert.match(
    styles,
    /html\s*\{[^}]*background-color:\s*var\(--bg\);/s,
  );
});
