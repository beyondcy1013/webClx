import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalCss = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);
const responsiveCss = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);
const terminalJs = readEntryScriptBundle("terminal.html");

function cssRuleBody(source, selectorPattern, message) {
  const match = source.match(new RegExp(`${selectorPattern}\\s*\\{([^}]*)\\}`));
  assert.ok(match, message);
  return match[1];
}

assert.match(
  terminalHtml,
  /id="terminal-agents-doc-name-input"[\s\S]*?placeholder="新建文档"/,
  "the compact create-document input should use a four-character placeholder",
);

const compactInputRule = cssRuleBody(
  terminalCss,
  String.raw`\.terminal-agents-doc-filter-input,\s*\.terminal-agents-doc-name-input`,
  "document filter and create inputs should share a compact rule",
);
assert.match(compactInputRule, /flex:\s*0 0 calc\(4em \+ 10px\);/);
assert.match(compactInputRule, /max-width:\s*calc\(4em \+ 10px\);/);
assert.match(compactInputRule, /padding-inline:\s*4px;/);

const compactToolbarRule = cssRuleBody(
  terminalCss,
  String.raw`\.terminal-agents-doc-toolbar`,
  "document toolbar should expose a dedicated compact rule",
);
assert.match(compactToolbarRule, /gap:\s*0\.25rem;/);
assert.match(compactToolbarRule, /flex:\s*0 0 auto;/);
assert.match(
  compactToolbarRule,
  /width:\s*max-content;/,
  "the compact document toolbar should size to its contents instead of wrapping a full-width flex item",
);

assert.match(
  responsiveCss,
  /@media \(max-width: 720px\)[\s\S]*?\.terminal-agents-doc-toolbar\s*\{[^}]*width:\s*100%;[^}]*max-width:\s*100%;[^}]*\}/,
  "the compact document toolbar should fall back to the dialog width on mobile",
);

assert.match(
  terminalHtml,
  /id="terminal-agents-doc-max-age-days"[\s\S]*?type="number"[\s\S]*?min="1"[\s\S]*?step="1"/,
  "document manager should expose a positive whole-day age filter",
);

assert.match(
  terminalJs,
  /const terminalAgentsDocMaxAgeDaysEl = document\.getElementById\("terminal-agents-doc-max-age-days"\);/,
  "terminal page should bind the document age filter",
);

assert.match(
  terminalJs,
  /terminalAgentsDocMaxAgeDaysEl\.addEventListener\("input", \(\) => \{[\s\S]*renderFilteredTerminalAgentsDocOptions\(\)/,
  "changing the document age should filter the loaded list immediately",
);

const filterFunction = terminalJs.match(
  /function terminalAgentsDocModifiedWithinDays\(documentInfo, days, nowSeconds\) \{[\s\S]*?\n\}/,
)?.[0];
assert.ok(filterFunction, "document age filtering should be implemented as testable pure logic");

const context = vm.createContext({});
vm.runInContext(filterFunction, context);

const nowSeconds = 2_000_000;
const withinDays = (documentInfo, days) =>
  vm.runInContext(
    `terminalAgentsDocModifiedWithinDays(${JSON.stringify(documentInfo)}, ${JSON.stringify(days)}, ${nowSeconds})`,
    context,
  );

assert.equal(withinDays({ modified: nowSeconds - 7 * 86_400 }, 7), true);
assert.equal(withinDays({ modified: nowSeconds - 7 * 86_400 - 1 }, 7), false);
assert.equal(withinDays({ modified: nowSeconds - 30 * 86_400 }, ""), true);
assert.equal(withinDays({ modified: nowSeconds - 30 * 86_400 }, 0), true);
assert.equal(withinDays({ modified: null }, 7), true);
