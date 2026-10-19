import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(
  new URL("../static/app-workspace-history.js", import.meta.url),
  "utf8",
);

assert.match(
  source,
  /function createWorkspaceHistoryPrimaryAction\(item, fallbackPath\) \{[\s\S]*?item\?\.type === "terminal"[\s\S]*?return createActionButton\("进入", \(\) => openSession\(item\.terminal\)[\s\S]*?if \(!item\?\.sessionId\)[\s\S]*?openWorkspaceHistorySessionDialog\(item, fallbackPath, "resume", button\)/,
  "workspace history should enter an active terminal instead of opening a new one",
);

console.log("workspace history active-terminal entry tests passed");
