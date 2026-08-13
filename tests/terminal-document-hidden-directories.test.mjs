import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalDocsRs = readFileSync(new URL("../src/terminal/docs.rs", import.meta.url), "utf8");

assert.match(
  terminalHtml,
  /id="terminal-agents-doc-show-hidden"[\s\S]*?type="checkbox"[\s\S]*?<span>显示隐藏目录<\/span>/,
  "document manager should expose an unchecked hidden-directory toggle",
);

assert.match(
  terminalJs,
  /function terminalAgentsDocShowHidden\(\)[\s\S]*terminalAgentsDocShowHiddenEl\?\.checked/,
  "document requests should read the hidden-directory toggle state",
);

assert.match(
  terminalJs,
  /async function fetchTerminalAgentsDocList\([\s\S]*showHidden = terminalAgentsDocShowHidden\(\)[\s\S]*show_hidden/,
  "document list requests should send the hidden-directory choice to the backend",
);

assert.match(
  terminalJs,
  /terminalAgentsDocShowHiddenEl\.addEventListener\("change", \(\) => \{[\s\S]*handleRefreshTerminalAgentsDocList\(\)/,
  "changing the hidden-directory toggle should reload the list instead of filtering an already-loaded list",
);

assert.match(
  terminalDocsRs,
  /struct TerminalAgentsDocListQuery \{[\s\S]*show_hidden: bool,[\s\S]*\}/,
  "the document list endpoint should accept a show_hidden query flag that defaults to false",
);

assert.match(
  terminalDocsRs,
  /pub async fn list_session_agents_docs\([\s\S]*Query\(query\): Query<TerminalAgentsDocListQuery>[\s\S]*list_terminal_doc_candidates\([\s\S]*query\.show_hidden,[\s\S]*&query\.recursive_dirs/,
  "the document list endpoint should pass show_hidden and recursive directories into traversal",
);

assert.match(
  terminalDocsRs,
  /fn should_skip_terminal_doc_directory\(name: &str, show_hidden: bool\)[\s\S]*!show_hidden && name\.starts_with\('\.'\)/,
  "directory traversal should skip dot-prefixed directories unless explicitly enabled",
);
