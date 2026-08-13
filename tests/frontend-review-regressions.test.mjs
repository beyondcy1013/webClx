import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const readStatic = (name) =>
  readFileSync(new URL(`../static/${name}`, import.meta.url), "utf8");

const pageHtml = new Map(
  ["index.html", "terminal.html", "agent.html", "login.html"].map((name) => [
    name,
    readStatic(name),
  ]),
);

test("pages discover split stylesheets without a CSS @import waterfall", () => {
  const stylesheets = [
    "styles-base.css",
    "styles-settings.css",
    "styles-auth.css",
    "styles-terminal.css",
    "styles-responsive.css",
  ];

  for (const [name, html] of pageHtml) {
    assert.doesNotMatch(
      html,
      /href="\/assets\/styles\.css(?:\?|\")/,
      `${name} should not load the @import entrypoint`,
    );

    let previousIndex = -1;
    for (const stylesheet of stylesheets) {
      const index = html.indexOf(`/assets/${stylesheet}`);
      assert.ok(index > previousIndex, `${name} should load ${stylesheet} in cascade order`);
      previousIndex = index;
    }
  }
});

test("terminal external scripts do not block HTML parsing", () => {
  const terminalHtml = pageHtml.get("terminal.html");
  const externalScripts = [
    ...terminalHtml.matchAll(/<script\s+([^>]*\bsrc="[^"]+"[^>]*)><\/script>/g),
  ];
  assert.ok(externalScripts.length > 0, "terminal page should load external scripts");

  for (const [, attributes] of externalScripts) {
    assert.match(attributes, /(?:^|\s)defer(?:\s|$)/, `script should use defer: ${attributes}`);
  }
});

test("workspace breadcrumb separators remain selectable for path copy", () => {
  const baseStyles = readStatic("styles-base.css");
  const separatorStart = baseStyles.indexOf(".browser-breadcrumb-separator");
  assert.ok(separatorStart >= 0, "breadcrumb separator style should exist");
  const separatorEnd = baseStyles.indexOf("}", separatorStart);
  assert.ok(separatorEnd > separatorStart, "breadcrumb separator style should be terminated");

  const separatorBlock = baseStyles.slice(separatorStart, separatorEnd + 1);
  assert.match(
    separatorBlock,
    /user-select\s*:\s*text/,
    "breadcrumb slashes should be selectable so copied paths retain / separators",
  );
});

test("workspace current path control copies the full path", () => {
  const appHtml = pageHtml.get("index.html");
  const appJs = readStatic("app.js");
  const coreBindingsJs = readStatic("app-core-event-bindings.js");

  assert.match(
    appHtml,
    /class="workspace-path-copy"[\s\S]*id="copy-current-path"[\s\S]*>\s*当前目录：\s*<\/button>[\s\S]*id="current-path"/,
    "workspace heading should keep the current path label adjacent to the path",
  );
  assert.match(
    appJs,
    /async function copyCurrentPath\(button\)[\s\S]*normalizeAbsolutePath\(state\.currentDirectory\?\.display_path[\s\S]*navigator\.clipboard\?\.writeText/,
    "current path copy should write the full normalized directory path",
  );
  assert.match(
    coreBindingsJs,
    /currentPathCopyButton\?\.addEventListener\("click", \(\) => \{[\s\S]*copyCurrentPath\(currentPathCopyButton\);/,
    "the current path label should trigger the copy action on click",
  );
});

test("quota rendering escapes service-controlled text", async () => {
  const quotaBody = { innerHTML: "" };
  const context = vm.createContext({
    URLSearchParams,
    console,
    requestJson: async () => {
      throw new Error('<img src=x onerror="globalThis.xss=true">');
    },
    terminalQuotaBodyEl: quotaBody,
    terminalQuotaKeyStatusEl: null,
    window: { clearTimeout, setTimeout },
  });
  vm.runInContext(readStatic("terminal-quota.js"), context);

  context.payload = {
    quota_limit: {
      limits: [
        {
          type: '<img src=x onerror="globalThis.xss=true">',
          percentage: '<svg onload="globalThis.xss=true">',
        },
      ],
    },
  };
  vm.runInContext("renderQuotaReport(payload)", context);
  assert.doesNotMatch(quotaBody.innerHTML, /<(?:img|svg)\b/i);
  assert.match(quotaBody.innerHTML, /&lt;img/);

  await vm.runInContext("refreshTerminalQuota()", context);
  assert.doesNotMatch(quotaBody.innerHTML, /<img\b/i);
  assert.match(quotaBody.innerHTML, /&lt;img/);
  assert.equal(context.xss, undefined);
});

test("compressible web responses enable Brotli and gzip compression", () => {
  const cargoToml = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8");
  const routesRs = readFileSync(new URL("../src/routes/mod.rs", import.meta.url), "utf8");
  const towerHttp = cargoToml.match(/tower-http\s*=\s*\{[^}]+\}/s)?.[0] || "";

  assert.match(towerHttp, /"compression-br"/);
  assert.match(towerHttp, /"compression-gzip"/);
  assert.match(routesRs, /tower_http::compression::CompressionLayer/);
  assert.match(
    routesRs,
    /\.layer\(CompressionLayer::new\(\)\.compress_when\(skip_file_downloads\)\)/,
  );
});
