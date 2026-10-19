import asyncio
from pathlib import Path

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"


async def main():
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page()
        await page.route(
            "http://webclx.test/device-attribute-test",
            lambda route: route.fulfill(
                status=200,
                content_type="text/html; charset=utf-8",
                body="<!doctype html><html><body><textarea id=probe></textarea></body></html>",
            ),
        )
        await page.goto("http://webclx.test/device-attribute-test")
        await page.evaluate(
            """() => {
              const DEVICE_ATTRIBUTE_REQUEST_PATTERN = /\\u001b\\[(?:>0?|0?)c/g;
              const DEVICE_ATTRIBUTE_RESPONSE_PREFIX_PATTERN = /^\\u001b(?:\\[(?:[?>]?(?:[0-9;]*)?)?)?$/;
              const DEVICE_ATTRIBUTE_RESPONSE_START_PATTERN = /^\\u001b\\[(?:\\??[0-9;]*)c/;
              const DEVICE_ATTRIBUTE_REQUEST_TAIL_LENGTH = 4;
              const MAX_PENDING_DEVICE_ATTRIBUTE_RESPONSES = 4;
              let terminalOutputTail = "";
              let terminalInputTail = "";
              let pendingDeviceAttributeResponses = 0;
              function terminalAsciiChunk(bytes) {
                let output = "";
                for (const byte of bytes) {
                  if (byte === 0x1b || byte === 0x0a || byte === 0x0d || byte === 0x09 || (byte >= 0x20 && byte <= 0x7e)) {
                    output += String.fromCharCode(byte);
                  }
                }
                return output;
              }
              function noteTerminalAutoResponseQueries(bytes) {
                const asciiChunk = terminalAsciiChunk(bytes);
                if (!asciiChunk) { terminalOutputTail = ""; return; }
                const scanWindow = terminalOutputTail + asciiChunk;
                let matchCount = 0;
                for (const match of scanWindow.matchAll(DEVICE_ATTRIBUTE_REQUEST_PATTERN)) {
                  const matchStart = match.index ?? -1;
                  const matchEnd = matchStart + match[0].length;
                  if (matchStart >= 0 && matchEnd > terminalOutputTail.length) matchCount += 1;
                }
                if (matchCount > 0) {
                  pendingDeviceAttributeResponses = Math.min(pendingDeviceAttributeResponses + matchCount, MAX_PENDING_DEVICE_ATTRIBUTE_RESPONSES);
                }
                terminalOutputTail = scanWindow.slice(-DEVICE_ATTRIBUTE_REQUEST_TAIL_LENGTH);
              }
              function isPotentialDeviceAttributeResponsePrefix(data) {
                return DEVICE_ATTRIBUTE_RESPONSE_PREFIX_PATTERN.test(data);
              }
              function filterTerminalAutoResponse(data) {
                if (!data) return "";
                const combined = terminalInputTail + data;
                terminalInputTail = "";
                if (pendingDeviceAttributeResponses <= 0) return combined;
                let index = 0;
                let output = "";
                while (index < combined.length) {
                  const slice = combined.slice(index);
                  const matchedResponse = slice.match(DEVICE_ATTRIBUTE_RESPONSE_START_PATTERN);
                  if (matchedResponse && pendingDeviceAttributeResponses > 0) {
                    pendingDeviceAttributeResponses -= 1;
                    index += matchedResponse[0].length;
                    continue;
                  }
                  if (pendingDeviceAttributeResponses > 0 && isPotentialDeviceAttributeResponsePrefix(slice)) {
                    terminalInputTail = slice;
                    break;
                  }
                  output += combined[index];
                  index += 1;
                }
                return output;
              }
              window.noteTerminalAutoResponseQueries = noteTerminalAutoResponseQueries;
              window.filterTerminalAutoResponse = filterTerminalAutoResponse;
              window.pendingCount = () => pendingDeviceAttributeResponses;
              window.responseMatches = DEVICE_ATTRIBUTE_RESPONSE_START_PATTERN.test("\\u001b[0;276;0c");
            }"""
        )
        await page.evaluate(
            """() => {
              noteTerminalAutoResponseQueries(new TextEncoder().encode('\\u001b[c'));
              window.sent = [];
            }"""
        )
        for character in "\u001b[0;276;0c":
            result = await page.evaluate(
                "(data) => { const value = filterTerminalAutoResponse(data); window.sent.push(...value); return value; }",
                character,
            )
        result = await page.evaluate(
            "(data) => { const value = filterTerminalAutoResponse(data); window.sent.push(...value); return value; }",
            "a",
        )
        assert result == "a", result
        assert await page.evaluate("window.responseMatches"), "new-style DA response must match"
        assert await page.evaluate("window.pendingCount") == 0, "pending DA response should be consumed"
        await browser.close()


asyncio.run(main())
