import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const expectedLabels = [
  "终端管理",
  "工作区",
  "历史工作区",
  "Codex_API",
  "Claude_API",
  "设置",
  "远程桌面",
  "Agent",
  "Codex_OAuth",
  "归档列表",
  "编译产物",
];

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
  "../static/agent.html",
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
    expectedLabels,
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
  /\.terminal-page \.terminal-page-nav \{[\s\S]*?border: 0;[\s\S]*?border-radius: 0;[\s\S]*?background: transparent;[\s\S]*?backdrop-filter: none;/,
  "mobile terminal navigation must not restore the rounded outer frame",
);

console.log("top navigation order tests passed");
