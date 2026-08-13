from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
CHROME = Path("/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome")
SCREENSHOT = Path("/tmp/webclx-codex-status-compact-mobile.png")


def main() -> None:
    console_errors: list[str] = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=str(CHROME),
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = browser.new_page(viewport={"width": 420, "height": 740}, device_scale_factor=1)
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.set_content(
            """
            <!doctype html>
            <style>
              :root { --terminal-bg: #0b1110; --terminal-fg: #d5e2da; }
              body { margin: 0; padding: 12px; background: #eef3ef; }
              #host { width: 396px; height: 520px; background: var(--terminal-bg); }
            </style>
            <div id="host"></div>
            """
        )
        page.add_style_tag(path=str(ROOT / "static/vendor/xterm.css"))
        page.add_script_tag(path=str(ROOT / "static/vendor/xterm.js"))
        page.add_script_tag(path=str(ROOT / "static/terminal-codex-status-compact.js"))

        result = page.evaluate(
            r"""
            async () => {
              const lines = [
                "╭───────────────────────────────────────────────╮",
                "│  >_ OpenAI Codex (v0.144.5)                   │",
                "│                                               │",
                "│  Model:                gpt-5.6-sol (reasoning │",
                "│  Model provider:       sub2api_gpt-5.6_1M - h │",
                "│  Directory:            /home/…/stockScreener  │",
                "│  Permissions:          Full Access            │",
                "│  Agents.md:            /home/root/.codex/AGEN │",
                "│  Thread name:          扩展字段基础上注册为ds │",
                "│  Collaboration mode:   Default                │",
                "│  Session:              019f741e-6bb4-7a03-ac4 │",
                "│  Forked from:          019f73d6-ece8-72d0-add │",
                "│                                               │",
                "│  Token usage:          1.73M total  (1.61M in │",
                "│  Context window:       98% left (15.8K used / │",
                "│  Limits:               not available for this │",
                "╰───────────────────────────────────────────────╯",
              ];
              const status = {
                version: "0.144.5",
                model: "gpt-5.6-sol",
                reasoning_effort: "xhigh",
                summary_mode: "auto",
                cwd: "/home/codes/stockScreener",
                permission: "Full Access",
                collaboration_mode: "Default",
                session_id: "019f741e-6bb4-7a03-ac49-d28a60ef3765",
                forked_from: "019f73d6-ece8-72d0-addc-e74da1b25a1a",
                thread_name: "扩展字段基础上注册为dsl",
                agents_md: ["/home/root/.codex/AGENTS.md", "/home/codes/webClx/AGENTS.md"],
                token_usage: {
                  input_tokens: 1610000,
                  output_tokens: 45700,
                  total_tokens: 1730000,
                },
                context_window: {
                  used_tokens: 15800,
                  total_tokens: 1000000,
                  percent_left: 98,
                },
              };
              const term = new Terminal({
                cols: 49,
                rows: 30,
                fontFamily: '"IBM Plex Mono", monospace',
                fontSize: 12,
                lineHeight: 1.3,
                theme: { background: "#0b1110", foreground: "#d5e2da" },
              });
              term.open(document.querySelector("#host"));
              term.resize(49, 30);
              await new Promise((resolve) => term.write(lines.join("\r\n"), resolve));
              window.__term = term;
              window.__compactor = WebClxCodexStatusCompact.createTerminalCodexStatusCompactor({
                term,
                sessionId: "s-test",
                getSession: () => ({
                  codex_api_preset_name: "sub2api_gpt-5.6_1M",
                  codex_api_base_url: "http://192.168.3.2:18381/v1",
                }),
                requestJson: async () => ({ codex_status: status }),
                isActive: () => true,
              });
              document.querySelector("#host").addEventListener("click", () => term.focus());
              await new Promise((resolve) => setTimeout(resolve, 180));
              const overlay = document.querySelector(".terminal-codex-status-compact-overlay");
              const screen = document.querySelector(".xterm-screen");
              const overlayRect = overlay?.getBoundingClientRect();
              const screenRect = screen?.getBoundingClientRect();
              const rightBorderLefts = Array.from(
                document.querySelectorAll(".terminal-codex-status-compact-right-border"),
                (element) => element.getBoundingClientRect().left,
              );
              const horizontalRuleRects = Array.from(
                document.querySelectorAll(".terminal-codex-status-compact-horizontal-rule"),
                (element) => element.getBoundingClientRect(),
              );
              const dimensions = term._core._renderService.dimensions;
              const hitTarget = overlayRect
                ? document.elementFromPoint(
                    overlayRect.left + dimensions.actualCellWidth * 4,
                    overlayRect.top + dimensions.actualCellHeight * 4.5,
                  )
                : null;
              const bufferText = Array.from({ length: term.buffer.active.length }, (_, index) =>
                term.buffer.active.getLine(index)?.translateToString(true) || "",
              ).join("\n");
              return {
                overlayText: overlay?.textContent || "",
                overlayDisplay: overlay ? getComputedStyle(overlay).display : "missing",
                overlayPointerEvents: overlay ? getComputedStyle(overlay).pointerEvents : "missing",
                overlayUserSelect: overlay ? getComputedStyle(overlay).userSelect : "missing",
                hitInsideOverlay: Boolean(overlay && hitTarget && overlay.contains(hitTarget)),
                selectionGeometry: overlayRect
                  ? {
                      left: overlayRect.left,
                      top: overlayRect.top,
                      cellWidth: dimensions.actualCellWidth,
                      cellHeight: dimensions.actualCellHeight,
                    }
                  : null,
                contained:
                  Boolean(overlayRect && screenRect) &&
                  overlayRect.left >= screenRect.left - 1 &&
                  overlayRect.right <= screenRect.right + 1 &&
                  overlayRect.top >= screenRect.top - 1 &&
                  overlayRect.bottom <= screenRect.bottom + 1,
                bufferStillTruncated:
                  bufferText.includes("019f741e-6bb4-7a03-ac4") &&
                  !bufferText.includes("019f741e-6bb4-7a03-ac49-d28a60ef3765"),
                rightBorderDrift:
                  rightBorderLefts.length > 0
                    ? Math.max(...rightBorderLefts) - Math.min(...rightBorderLefts)
                    : 999,
                horizontalRuleCount: horizontalRuleRects.length,
                horizontalRuleWidthDrift:
                  horizontalRuleRects.length > 0
                    ? Math.max(...horizontalRuleRects.map((rect) => rect.width)) -
                      Math.min(...horizontalRuleRects.map((rect) => rect.width))
                    : 999,
                horizontalRulesContained:
                  Boolean(overlayRect) &&
                  horizontalRuleRects.every(
                    (rect) =>
                      rect.left >= overlayRect.left - 1 && rect.right <= overlayRect.right + 1,
                  ),
              };
            }
            """
        )
        page.evaluate(
            """
            () => {
              window.__compactCopyEventText = null;
              document.addEventListener(
                "copy",
                () => {
                  window.__compactCopyEventText = window.getSelection()?.toString() || "";
                },
                { once: true },
              );
            }
            """
        )
        geometry = result["selectionGeometry"]
        page.mouse.move(
            geometry["left"] + geometry["cellWidth"] * 1.5,
            geometry["top"] + geometry["cellHeight"] * 3.4,
        )
        page.mouse.down()
        page.mouse.move(
            geometry["left"] + geometry["cellWidth"] * 40,
            geometry["top"] + geometry["cellHeight"] * 12.6,
            steps=12,
        )
        page.mouse.up()
        selection_before_refresh = page.evaluate(
            "window.getSelection()?.toString() || ''"
        )
        page.evaluate("window.__compactor.refresh()")
        page.wait_for_timeout(100)
        page.keyboard.press("Control+C")
        result.update(
            page.evaluate(
                """
                () => ({
                  domSelection: window.getSelection()?.toString() || "",
                  copyEventText: window.__compactCopyEventText,
                  xtermSelection: window.__term.getSelection(),
                })
                """
            )
        )
        result["selectionBeforeRefresh"] = selection_before_refresh
        page.screenshot(path=str(SCREENSHOT), full_page=True)
        browser.close()

    assert result["overlayDisplay"] == "block", result
    assert result["overlayPointerEvents"] == "auto", result
    assert result["overlayUserSelect"] == "text", result
    assert result["hitInsideOverlay"], result
    assert "Model    │ gpt-5.6-sol | xhigh | auto" in result["selectionBeforeRefresh"], result
    assert "Model    │ gpt-5.6-sol | xhigh | auto" in result["domSelection"], result
    assert "Agents   │ /home/root/.codex/AGENTS.md" in result["domSelection"], result
    assert "Model:" not in result["domSelection"], result
    assert result["copyEventText"] == result["domSelection"], result
    assert result["xtermSelection"] == "", result
    assert "019f741e-6bb4-7a03-ac49-d28a60ef3765" in result["overlayText"], result
    assert "├─────────┬─────────────────────────────────────┤" in result["overlayText"], result
    assert "Model    │ gpt-5.6-sol | xhigh | auto" in result["overlayText"], result
    assert "Agents   │ /home/root/.codex/AGENTS.md" in result["overlayText"], result
    assert "         │ /home/codes/webClx/AGENTS.md" in result["overlayText"], result
    assert "Context  │ 98% left | 15.8K / 1.00M" in result["overlayText"], result
    assert "Model:" not in result["overlayText"], result
    assert result["contained"], result
    assert result["bufferStillTruncated"], result
    assert result["rightBorderDrift"] < 0.25, result
    assert result["horizontalRuleCount"] == 12, result
    assert result["horizontalRuleWidthDrift"] < 0.25, result
    assert result["horizontalRulesContained"], result
    assert not console_errors, console_errors
    print(f"codex status browser test passed: {SCREENSHOT}")


if __name__ == "__main__":
    main()
