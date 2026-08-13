import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readEntryScriptBundle("index.html");
const terminalRoutesRs = readFileSync(new URL("../src/routes/terminal.rs", import.meta.url), "utf8");
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalManagerRs = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");
const tmuxRs = readFileSync(new URL("../src/terminal/tmux.rs", import.meta.url), "utf8");

assert.match(
  indexHtml,
  /id="sessions-search-form"[\s\S]*id="sessions-search-input"[\s\S]*aria-label="搜索终端输出"[\s\S]*placeholder="搜索"[\s\S]*id="sessions-search-submit"[\s\S]*>搜索<\/button>[\s\S]*id="sessions-search-clear"[\s\S]*>清除<\/button>/,
  "active terminal tab should expose a terminal output search form",
);

assert.match(
  indexHtml,
  /<th>标题<\/th>\s*<th>匹配<\/th>/,
  "active terminal table should show a match column",
);

assert.match(
  appJs,
  /async function searchSessionsOutput\(query\)[\s\S]*\/api\/terminal\/sessions\/search\?q=\$\{encodeURIComponent\(normalizedQuery\)\}[\s\S]*state\.sessionSearchResults/,
  "frontend should call the terminal output search API and store matches",
);

assert.match(
  appJs,
  /function sessionFromSearchResult\(result\)[\s\S]*session_name[\s\S]*display_path/,
  "search results should be able to render before the full session list finishes loading",
);

assert.match(
  appJs,
  /function visibleSessionsForSearch\(resultMap\)[\s\S]*state\.sessionSearchQuery[\s\S]*state\.sessionSearchResults[\s\S]*sessionFromSearchResult\(result\)[\s\S]*resultMap\.has\(session\.id\)/,
  "active terminal rows should filter to matching sessions while a search is active",
);

assert.match(
  appJs,
  /function sessionSearchMatchLabel\(result\)[\s\S]*line_number[\s\S]*match_count[\s\S]*result\.line/,
  "search results should display the matched output line and count",
);

assert.match(
  terminalRoutesRs,
  /"\/api\/terminal\/sessions\/search"[\s\S]*get\(terminal::search_sessions\)/,
  "router should expose the terminal output search endpoint",
);

assert.match(
  terminalRs,
  /pub async fn search_sessions\([\s\S]*SearchSessionsQuery[\s\S]*search_active_session_output/,
  "terminal API should delegate output searches to the terminal manager",
);

assert.match(
  terminalManagerRs,
  /pub fn search_active_session_output\([\s\S]*filter\(\|session\| !session\.idle\)[\s\S]*capture_tmux_text_pane_snapshot[\s\S]*find_terminal_output_match/,
  "terminal manager should search active session output snapshots",
);

assert.match(
  tmuxRs,
  /pub\(super\) fn capture_tmux_text_pane_snapshot\(session_id: &str\)[\s\S]*capture_tmux_pane_snapshot_from\(session_id, "-", false\)/,
  "search should capture readable tmux text without ANSI escape sequences",
);
