import os
from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
CHROME = Path("/home/third_party/browser-tools/bin/chromium")
SCREENSHOT = Path("/tmp/webclx-codex-status-output.png")
DEPLOYED_ASSET_URL = os.environ.get("WEBCLX_CODEX_STATUS_OUTPUT_URL", "").strip()


def main() -> None:
    console_errors: list[str] = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=str(CHROME),
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = browser.new_page(viewport={"width": 720, "height": 640})
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
              body { margin: 0; padding: 12px; background: #eef3ef; }
              #host { width: 620px; height: 520px; background: #0b1110; }
            </style>
            <div id="host"></div>
            """
        )
        page.add_style_tag(path=str(ROOT / "static/vendor/xterm.css"))
        page.add_script_tag(path=str(ROOT / "static/vendor/xterm.js"))
        if DEPLOYED_ASSET_URL:
            page.add_script_tag(url=DEPLOYED_ASSET_URL)
        else:
            page.add_script_tag(path=str(ROOT / "static/terminal-codex-status-output.js"))

        result = page.evaluate(
            r"""
            async () => {
              const source = [
                "  \u001b[2m\u001b[39m  ╭───────────────────────────────────────────────╮",
                "│  >_ \u001b[0;1mOpenAI Codex\u001b[0;2m (v0.145.0)                   │",
                "│                                               │",
                "│  Model:                \u001b[0mgpt-5.6-sol\u001b[2m (reasoning │",
                "│  Model provider:       \u001b[0msub2api_gpt-5.6_1M - h\u001b[2m │",
                "│  Directory:            \u001b[0m/srv/alpha, /srv/beta\u001b[2m │",
                "│  Permissions:          \u001b[0mFull Access\u001b[2m            │",
                "│  Agents.md:            \u001b[0m/home/root/.codex/AGEN\u001b[2m │",
                "│  Collaboration mode:   \u001b[0mDefault\u001b[2m                │",
                "│  Session:              \u001b[0m019f9a23-21c5-7bc1-b1f\u001b[2m │",
                "│                                               │",
                "│  Token usage:          \u001b[0m1.43M total\u001b[2m (1.34M in │",
                "│  Context window:       \u001b[0m55% left\u001b[2m (122K used /  │",
                "│  Limits:               not available for this │",
                "╰───────────────────────────────────────────────╯",
              ].join("\r\n");
              const encoder = new TextEncoder();
              const transformer = WebClxCodexStatusOutput.createCodexStatusOutputTransformer();
              const bytes = encoder.encode(source);
              const chunks = [];
              for (let offset = 0; offset < bytes.length; offset += 7) {
                const output = transformer.transform(bytes.slice(offset, offset + 7));
                if (output.length > 0) chunks.push(output);
              }
              const pending = transformer.flush();
              if (pending.length > 0) chunks.push(pending);
              const size = chunks.reduce((total, chunk) => total + chunk.length, 0);
              const output = new Uint8Array(size);
              let outputOffset = 0;
              chunks.forEach((chunk) => {
                output.set(chunk, outputOffset);
                outputOffset += chunk.length;
              });

              const term = new Terminal({
                cols: 49,
                rows: 30,
                scrollback: 200,
                fontFamily: 'monospace',
                fontSize: 13,
                theme: { background: '#0b1110', foreground: '#d5e2da' },
              });
              term.open(document.querySelector('#host'));
              await new Promise((resolve) => term.write(output, resolve));
              await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

              const lines = Array.from({ length: term.buffer.active.length }, (_, index) =>
                term.buffer.active.getLine(index)?.translateToString(true) || '',
              );
              const start = lines.findIndex((line) => line.startsWith('╭'));
              const end = lines.findIndex((line, index) => index >= start && line.startsWith('╰'));
              const canvas = document.querySelector('.xterm-screen canvas');
              let nonBlankPixels = 0;
              if (canvas) {
                const context = canvas.getContext('2d', { willReadFrequently: true });
                const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
                for (let index = 3; index < pixels.length; index += 4) {
                  if (pixels[index] > 0) nonBlankPixels += 1;
                }
              }
              return {
                lines,
                blockLines: start >= 0 && end >= start ? lines.slice(start, end + 1) : [],
                overlayCount: document.querySelectorAll('[class*="overlay"]').length,
                nonBlankPixels,
              };
            }
            """
        )
        page.screenshot(path=str(SCREENSHOT), full_page=True)
        browser.close()

    block_lines = result["blockLines"]
    assert console_errors == [], console_errors
    assert len(block_lines) == 15, block_lines
    assert any("│Model: gpt-5.6-sol" in line for line in block_lines), block_lines
    assert any("│Access: Full Access" in line for line in block_lines), block_lines
    assert any("│Mode: Default" in line for line in block_lines), block_lines
    assert all("Collaboration mode:" not in line for line in block_lines), block_lines
    assert any("│Dir: /srv/alpha," in line for line in block_lines), block_lines
    assert any("│/srv/beta" in line for line in block_lines), block_lines
    assert all("Model:                " not in line for line in result["lines"]), result["lines"]
    assert len({len(line) for line in block_lines}) == 1, block_lines
    assert len(block_lines[0]) < 49, block_lines
    assert block_lines[0].startswith("╭"), block_lines
    assert result["overlayCount"] == 0, result
    assert result["nonBlankPixels"] > 0, result
    print(
        {
            "rows": len(block_lines),
            "columns": len(block_lines[0]),
            "overlay_count": result["overlayCount"],
            "non_blank_pixels": result["nonBlankPixels"],
            "screenshot": str(SCREENSHOT),
            "asset": DEPLOYED_ASSET_URL or "source",
        }
    )


if __name__ == "__main__":
    main()
