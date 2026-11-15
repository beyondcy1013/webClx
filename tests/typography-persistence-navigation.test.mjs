import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const pages = [
  "../static/index.html",
  "../static/terminal.html",
  "../static/agent.html",
  "../static/agent-profiles.html",
  "../static/collaboration.html",
  "../static/login.html",
];

for (const pagePath of pages) {
  const content = readFileSync(new URL(pagePath, import.meta.url), "utf8");
  assert.match(
    content,
    /<head>[\s\S]*?localStorage\.getItem\(["']webclx:font-size-tiers["']\)[\s\S]*?<\/head>/,
    `${pagePath} must restore typography from localStorage inside <head> before first paint`,
  );
  assert.match(
    content,
    /--font-size-tier-1/,
    `${pagePath} early typography script must set --font-size-tier-1`,
  );
  assert.match(
    content,
    /--font-size-tier-4/,
    `${pagePath} early typography script must set --font-size-tier-4`,
  );
}

const css = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");
assert.match(
  css,
  /html\s*\{[\s\S]*?-webkit-text-size-adjust:\s*100%;[\s\S]*?text-size-adjust:\s*100%;/,
  "styles-base.css must prevent mobile text autosizing / font boosting",
);

const terminalShellSettings = readFileSync(
  new URL("../static/terminal-shell-settings.js", import.meta.url),
  "utf8",
);
assert.match(
  terminalShellSettings,
  /function persistTypographySettings\(tiers\)/,
  "terminal-shell-settings.js must define persistTypographySettings",
);
assert.match(
  terminalShellSettings,
  /persistTypographySettings\(tiers\)/,
  "terminal-shell-settings.js must persist tiers on applyTypographySettings",
);

const appSettingsFormatters = readFileSync(
  new URL("../static/app-settings-formatters.js", import.meta.url),
  "utf8",
);
assert.match(
  appSettingsFormatters,
  /function persistTypographySettings\(tiers\)/,
  "app-settings-formatters.js must define persistTypographySettings",
);
assert.match(
  appSettingsFormatters,
  /persistTypographySettings\(tiers\)/,
  "app-settings-formatters.js must persist tiers on applyTypographySettings",
);

const terminalSettings = readFileSync(
  new URL("../static/terminal-settings.js", import.meta.url),
  "utf8",
);
assert.match(
  terminalSettings,
  /const FONT_SIZE_TIERS_STORAGE_KEY = "webclx:font-size-tiers";/,
  "terminal-settings.js must export FONT_SIZE_TIERS_STORAGE_KEY",
);
assert.match(
  terminalSettings,
  /function readStoredFontSizeTiers\(/,
  "terminal-settings.js must define readStoredFontSizeTiers",
);

const terminalJs = readFileSync(
  new URL("../static/terminal.js", import.meta.url),
  "utf8",
);
assert.match(
  terminalJs,
  /event\.key === FONT_SIZE_TIERS_STORAGE_KEY/,
  "terminal.js must synchronize font sizes on storage event",
);

const appSettingsEventBindings = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);
assert.match(
  appSettingsEventBindings,
  /event\.key === "webclx:font-size-tiers"/,
  "app-settings-event-bindings.js must synchronize font sizes on storage event",
);

console.log("typography persistence and navigation tests passed");
