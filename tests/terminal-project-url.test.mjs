import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const start = source.indexOf("function resolveProjectWebUrl(");
const end = source.indexOf("\nasync function openProjectUrl()", start);
assert.ok(start >= 0 && end > start, "project URL helper should be extractable");

const resolveProjectWebUrl = Function(
  `${source.slice(start, end)}; return resolveProjectWebUrl;`,
)();
const locationLike = {
  protocol: "https:",
  hostname: "dev.example.com",
  origin: "https://dev.example.com:11111",
};

assert.equal(
  resolveProjectWebUrl({ web: { port: 4173 } }, locationLike),
  "https://dev.example.com:4173/",
  "port config should preserve the browser protocol and hostname",
);
assert.equal(
  resolveProjectWebUrl({ web: { port: 8080, scheme: "http", path: "/admin" } }, locationLike),
  "http://dev.example.com:8080/admin",
  "port config should support an explicit scheme and path",
);
assert.equal(
  resolveProjectWebUrl({ web: { url: "/preview" } }, locationLike),
  "https://dev.example.com:11111/preview",
  "relative URL config should resolve against the current webClx origin",
);
assert.equal(
  resolveProjectWebUrl({ web: { url: "javascript:alert(1)" } }, locationLike),
  "",
  "unsafe URL protocols should be rejected",
);
assert.equal(
  resolveProjectWebUrl({ web: { port: 70000 } }, locationLike),
  "",
  "invalid ports should be rejected",
);
assert.equal(
  resolveProjectWebUrl({}, locationLike),
  "",
  "missing web config should not invent a project URL",
);

assert.match(
  source,
  /const popup = window\.open\("", "_blank"\);[\s\S]*?popup\.location\.replace\(projectUrl\)/,
  "project URL should reserve a tab during the user gesture before loading config asynchronously",
);
