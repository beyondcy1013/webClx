import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const expectedByPath = {
  "../static/index.html": [
    "终端管理",
    "工作区",
    "历史工作区",
    "LLM_API",
    "Claude_API",
    "设置",
    "DeepSeek Agent",
    "智能体协作",
    "归档列表",
    "编译产物",
  ],
  "../static/terminal.html": [
    "终端管理",
    "工作区",
    "历史工作区",
    "LLM_API",
    "Claude_API",
    "设置",
    "DeepSeek Agent",
    "智能体协作",
    "服务器",
    "归档列表",
    "编译产物",
  ],
  "../static/agent-profiles.html": [
    "终端管理",
    "工作区",
    "历史工作区",
    "LLM_API",
    "Claude_API",
    "设置",
    "DeepSeek Agent",
    "智能体协作",
    "归档列表",
    "编译产物",
  ],
};

function navigationItems(relativePath) {
  const html = readFileSync(new URL(relativePath, import.meta.url), "utf8");
  const navigation = html.match(
    /<(?:div|nav)[^>]+class="[^"]*page-tabs[^"]*"[^>]*>([\s\S]*?)<\/(?:div|nav)>/,
  );
  assert.ok(navigation, `${relativePath} should expose top-level navigation`);

  return Array.from(
    navigation[1].matchAll(/<(?:a|button)\b([^>]*)>([\s\S]*?)<\/(?:a|button)>/g),
    (match) => ({
      attributes: match[1],
      label: match[2].replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim(),
    }),
  );
}

for (const relativePath of [
  "../static/index.html",
  "../static/terminal.html",
  "../static/agent-profiles.html",
]) {
  const items = navigationItems(relativePath);
  assert.equal(items[0]?.label, "终端管理", `${relativePath} should lead with 终端管理`);
  assert.match(
    items[0]?.attributes || "",
    /href="\/terminal"/,
    `${relativePath} 终端管理 should open the terminal workspace`,
  );
  assert.deepEqual(
    items.map(({ label }) => label),
    expectedByPath[relativePath],
    `${relativePath} should expose the shared top-level navigation order`,
  );
}

const terminalItems = navigationItems("../static/terminal.html");
assert.match(
  terminalItems[0].attributes,
  /class="[^"]*active[^"]*"/,
  "terminal page should mark 终端管理 as active",
);
assert.doesNotMatch(
  terminalItems[0].attributes,
  /data-home-path=/,
  "terminal page should keep its current-page link from creating a path-based session",
);

const sharedStyles = readFileSync(
  new URL("../static/styles-base.css", import.meta.url),
  "utf8",
);
const responsiveStyles = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);
assert.match(
  sharedStyles,
  /\.page-tabs \.tab-button \{[\s\S]*?border: 0;[\s\S]*?border-radius: 0;[\s\S]*?background: transparent;[\s\S]*?color: var\(--muted\);/,
  "top-level navigation should read as one button group with quiet default buttons",
);
assert.match(
  sharedStyles,
  /\.page-tabs \.tab-button\.active,[\s\S]*?\[aria-current="page"\][\s\S]*?background: rgba\(15, 122, 98, 0\.16\);[\s\S]*?color: var\(--accent-strong\);[\s\S]*?box-shadow: none;/,
  "top-level navigation should distinguish the current button without a rounded frame",
);
assert.match(
  sharedStyles,
  /\.page-tabs \{[\s\S]*?border: 0;[\s\S]*?border-radius: 0;[\s\S]*?background: transparent;/,
  "the navigation button group must not be wrapped in a rounded table",
);
assert.match(
  sharedStyles,
  /\.browser-topbar \{[\s\S]*?border: 0;[\s\S]*?border-radius: 0;[\s\S]*?background: transparent;[\s\S]*?backdrop-filter: none;/,
  "the sticky navigation carrier must remain visually unframed",
);
assert.match(
  responsiveStyles,
  /\.terminal-page \.terminal-page-nav \{[\s\S]*?border: 0;[\s\S]*?border-radius: 0;[\s\S]*?background: var\(--panel-opaque\);[\s\S]*?backdrop-filter: none;/,
  "mobile terminal navigation must keep an opaque scroll-covering layer without restoring the rounded outer frame",
);
assert.match(
  sharedStyles,
  /\.terminal-page-nav \{[\s\S]*?background: var\(--panel-opaque\);[\s\S]*?isolation: isolate;/,
  "terminal navigation must remain opaque and isolated while terminal output scrolls underneath it",
);
assert.match(
  sharedStyles,
  /:root\[data-theme="dark"\] \.browser-topbar\.terminal-page-nav[\s\S]*?background: var\(--panel-opaque\);/,
  "terminal navigation must override the dark-theme transparent browser-topbar rule with higher specificity",
);
assert.match(
  responsiveStyles,
  /:root\[data-theme="dark"\] \.terminal-page \.terminal-page-nav[\s\S]*?background: var\(--panel-opaque\);/,
  "mobile terminal navigation must keep an opaque background under dark theme",
);

console.log("top navigation order tests passed");
