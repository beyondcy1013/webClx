import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workspaceBrowserJs = readFileSync(
  new URL("../static/app-workspace-browser.js", import.meta.url),
  "utf8",
);
const baseCss = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");
const responsiveCss = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);

test("workspace designate buttons use the captured button as dialog trigger", () => {
  assert.match(
    workspaceBrowserJs,
    /let parentDesignateAction;[\s\S]*?parentDesignateAction = createActionButton\("指定", \(\) => {[\s\S]*?openWorkspaceDesignatePresetDialog\(directory\.parent_path, parentDesignateAction\)/,
  );
  assert.match(
    workspaceBrowserJs,
    /let designateAction;[\s\S]*?designateAction = createActionButton\("指定", \(\) => {[\s\S]*?openWorkspaceDesignatePresetDialog\(entry\.path, designateAction\)/,
  );
  assert.doesNotMatch(
    workspaceBrowserJs,
    /createActionButton\("指定", \(event\) => {[\s\S]*?event\.currentTarget/,
  );
});

test("workspace action column contains all directory actions without covering favorites", () => {
  const desktopActionColumnRule = /\.file-browser-table th:nth-child\(1\),\s*\n\s*\.file-browser-table td:nth-child\(1\)\s*\{[^}]*width:\s*116px;/;
  const mobileActionColumnRule = /\.file-browser-table th:first-child,\s*\n\s*\.file-browser-table \.file-browser-action-cell\s*\{[^}]*width:\s*128px;/;

  assert.match(baseCss, desktopActionColumnRule);
  assert.match(responsiveCss, mobileActionColumnRule);
  assert.match(
    responsiveCss,
    /\.file-browser-action-cell \.actions\s*\{[^}]*display:\s*grid;[^}]*grid-template-columns:\s*repeat\(3,\s*minmax\(0,\s*1fr\)\);[^}]*gap:\s*4px;/s,
  );
});
