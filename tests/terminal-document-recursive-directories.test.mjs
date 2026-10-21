import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalStyles = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");
const responsiveStyles = readFileSync(new URL("../static/styles-responsive.css", import.meta.url), "utf8");
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalDocsRs = readFileSync(new URL("../src/terminal/docs.rs", import.meta.url), "utf8");

const scanOptions = terminalHtml.match(
  /<details id="terminal-agents-doc-scan-options"[\s\S]*?<\/details>/,
)?.[0] || "";

assert.match(scanOptions, /<summary>扫描选项<\/summary>/, "scan options should be collapsed behind a summary");
assert.match(
  scanOptions,
  /id="terminal-agents-doc-recursive-directories"[\s\S]*?value="docs"/,
  "recursive directory names should default to docs",
);
assert.match(
  scanOptions,
  /<span>目录用逗号隔<\/span>/,
  "recursive directory controls should explain comma-separated entries inline",
);
assert.match(
  scanOptions,
  /id="terminal-agents-doc-recursive-directories"[\s\S]*?title="多个目录使用英文逗号分隔，例如 docs, PRD"/,
  "recursive directory input should expose a concrete comma-separated example",
);
assert.match(
  scanOptions,
  /id="terminal-agents-doc-recursive-directories"[\s\S]*id="terminal-agents-doc-show-hidden"/,
  "the hidden-directory toggle should live inside the scan options after the recursive directory list",
);
assert.match(
  terminalStyles,
  /\.terminal-agents-doc-scan-options:not\(\[open\]\)\s*>\s*\.terminal-agents-doc-scan-options-body\s*\{[\s\S]*?display:\s*none;/,
  "closed scan options should hide author-styled flex content",
);
assert.match(
  terminalStyles,
  /\.terminal-agents-doc-scan-options\[open\]\s*\{[\s\S]*?display:\s*flex;[\s\S]*?align-items:\s*center;[\s\S]*?\.terminal-agents-doc-scan-options\[open\]\s*>\s*\.terminal-agents-doc-scan-options-body\s*\{[\s\S]*?flex-wrap:\s*nowrap;[\s\S]*?padding-top:\s*0;/,
  "expanded scan options should stay on one compact row",
);
assert.match(
  terminalStyles,
  /\.terminal-agents-doc-scan-options\[open\][\s\S]*?\.terminal-agents-doc-recursive-field\s*\{[\s\S]*?min-width:\s*0;[\s\S]*?\.terminal-agents-doc-scan-options\[open\][\s\S]*?\.terminal-agents-doc-recursive-directories\s*\{[\s\S]*?min-width:\s*0;/,
  "the recursive directory field should shrink enough to keep the options on one row",
);
assert.match(
  responsiveStyles,
  /@media \(max-width: 720px\) \{[\s\S]*?\.terminal-agents-doc-form \.panel-head\.wide\s*\{[\s\S]*?flex-direction:\s*column;[\s\S]*?align-items:\s*stretch;/,
  "the document dialog header should stack on mobile instead of overflowing horizontally",
);
assert.match(
  responsiveStyles,
  /@media \(max-width: 720px\) \{[\s\S]*?\.terminal-agents-doc-form\s*\{[\s\S]*?height:\s*min\(86vh,\s*920px\);[\s\S]*?\.terminal-agents-doc-editor\s*\{[\s\S]*?min-height:\s*0;/,
  "the mobile document editor should shrink inside a stable dialog height without covering its actions",
);

assert.match(
  terminalJs,
  /const terminalAgentsDocRecursiveDirectoriesEl = document\.getElementById\("terminal-agents-doc-recursive-directories"\);/,
  "the terminal page should bind the recursive directory list",
);
assert.match(
  terminalJs,
  /function terminalAgentsDocRecursiveDirectories\(\)[\s\S]*terminalAgentsDocRecursiveDirectoriesEl\?\.value[\s\S]*return value \|\| "docs";/,
  "an empty recursive directory list should fall back to docs",
);
assert.match(
  terminalJs,
  /async function fetchTerminalAgentsDocList\([\s\S]*recursive_dirs: terminalAgentsDocRecursiveDirectories\(\)/,
  "document list requests should send the recursive directory names",
);
assert.match(
  terminalJs,
  /async function loadTerminalAgentsDoc\([\s\S]*recursive_dirs: terminalAgentsDocRecursiveDirectories\(\)/,
  "document read requests should preserve the recursive directory names",
);
assert.match(
  terminalJs,
  /async function saveTerminalAgentsDoc\([\s\S]*recursive_dirs: terminalAgentsDocRecursiveDirectories\(\)/,
  "document save requests should preserve the recursive directory names",
);

assert.match(
  terminalRs,
  /struct TerminalAgentsDocSaveRequest \{[\s\S]*recursive_dirs: String,[\s\S]*\}/,
  "document save requests should accept recursive directory names",
);
assert.match(
  terminalRs,
  /struct TerminalAgentsDocPathQuery \{[\s\S]*recursive_dirs: String,[\s\S]*\}/,
  "document read requests should accept recursive directory names",
);
assert.match(
  terminalDocsRs,
  /struct TerminalAgentsDocListQuery \{[\s\S]*recursive_dirs: String,[\s\S]*\}/,
  "document list requests should accept recursive directory names",
);
assert.match(
  terminalDocsRs,
  /fn parse_recursive_doc_directories\([\s\S]*\.to_lowercase\(\)[\s\S]*"docs"\.to_string\(\)/,
  "recursive directory names should be matched case-insensitively and default to docs",
);
assert.match(
  terminalDocsRs,
  /collect_terminal_doc_entries\([\s\S]*session_dir,[\s\S]*false,[\s\S]*&recursive_directories/,
  "the session root should not recurse unless a child directory is explicitly listed",
);
assert.match(
  terminalDocsRs,
  /fn should_recurse_terminal_doc_directory\([\s\S]*recursive \|\| recursive_directories\.contains\(&name\.to_lowercase\(\)\)/,
  "only listed root directories should start recursive traversal, ignoring name case",
);
