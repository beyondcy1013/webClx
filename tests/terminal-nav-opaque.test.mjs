import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright";

const chromePath =
  process.env.CHROME_BIN ||
  process.env.CHROMIUM_BIN ||
  "/home/codes/third_party/browser-tools/bin/chrome";

assert.ok(existsSync(chromePath), `Chrome binary should exist at ${chromePath}`);

const browser = await chromium.launch({
  headless: true,
  executablePath: chromePath,
  args: ["--no-sandbox", "--disable-gpu"],
});

try {
  const viewports = [
    { width: 1280, height: 800, name: "desktop" },
    { width: 375, height: 667, name: "mobile" },
    { width: 720, height: 1280, name: "tablet" },
  ];

  for (const vp of viewports) {
    const page = await browser.newPage({ viewport: { width: vp.width, height: vp.height } });

    await page.route("**/assets/**", (route) => {
      const url = new URL(route.request().url());
      const relative = url.pathname.replace(/^\/assets\//, "");
      const localPath = join("/home/bin/webclx/static", relative);
      if (existsSync(localPath)) {
        route.fulfill({
          contentType: relative.endsWith(".css") ? "text/css" : "application/javascript",
          body: readFileSync(localPath),
        });
      } else {
        route.continue();
      }
    });

    await page.goto("file:///home/bin/webclx/static/terminal.html");

    for (const theme of ["dark", "light"]) {
      await page.evaluate((th) => {
        document.documentElement.dataset.theme = th;
      }, theme);

      const navStyle = await page.evaluate(() => {
        const el = document.querySelector(".terminal-page-nav");
        const cs = window.getComputedStyle(el);
        return {
          backgroundColor: cs.backgroundColor,
          position: cs.position,
          zIndex: cs.zIndex,
          top: cs.top,
          isolation: cs.isolation,
        };
      });

      assert.equal(navStyle.position, "sticky", `${vp.name} [${theme}] nav should be sticky`);
      assert.equal(navStyle.zIndex, "24", `${vp.name} [${theme}] nav z-index should be 24`);
      assert.equal(navStyle.top, "0px", `${vp.name} [${theme}] nav top should be 0px`);
      assert.notEqual(
        navStyle.backgroundColor,
        "rgba(0, 0, 0, 0)",
        `${vp.name} [${theme}] nav background must NOT be transparent`,
      );

      if (theme === "dark") {
        assert.equal(
          navStyle.backgroundColor,
          "rgb(12, 18, 22)",
          `${vp.name} dark theme nav background should equal --panel-opaque (rgb(12, 18, 22))`,
        );
      } else {
        assert.equal(
          navStyle.backgroundColor,
          "rgb(251, 247, 239)",
          `${vp.name} light theme nav background should equal --panel-opaque (rgb(251, 247, 239))`,
        );
      }
    }

    // Test scroll overlap: verify terminal content scrolling behind nav is covered
    await page.evaluate(() => {
      document.documentElement.dataset.theme = "dark";
      const host = document.getElementById("terminal-host");
      if (host) {
        host.style.height = "2000px";
      }
      window.scrollTo(0, 200);
    });

    await page.waitForTimeout(50);

    const coverage = await page.evaluate(() => {
      const nav = document.querySelector(".terminal-page-nav");
      const rect = nav.getBoundingClientRect();
      const points = [];
      const step = Math.max(Math.floor(rect.width / 10), 20);
      for (let x = rect.left + 10; x < rect.right - 10; x += step) {
        const y = rect.top + rect.height / 2;
        const el = document.elementFromPoint(x, y);
        const coveredByNav = nav.contains(el) || el === nav;
        points.push({ x, y, coveredByNav, tag: el?.tagName });
      }
      return { rect, points };
    });

    assert.ok(
      coverage.points.every((p) => p.coveredByNav),
      `${vp.name} all points across terminal nav bar must hit nav bar elements, not scrolled content`,
    );

    await page.close();
  }

  console.log("terminal-nav-opaque browser tests passed");
} finally {
  await browser.close();
}
