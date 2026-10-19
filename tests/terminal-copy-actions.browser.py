import asyncio
from pathlib import Path

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
WRONG_SELECTION = "Already viewing the wrong terminal selection"
TERMINAL_NAME = "outlook-mail-reference_1"
SESSION_ID = "019ff696-55e1-7f23-ab83-3d0dcf6e321f"


async def main():
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        context = await browser.new_context()
        page = await context.new_page()
        await page.route(
            "http://webclx.test/clipboard-test",
            lambda route: route.fulfill(
                status=200,
                content_type="text/html; charset=utf-8",
                body="<!doctype html><html><body></body></html>",
            ),
        )
        await page.goto("http://webclx.test/clipboard-test")

        await page.add_script_tag(path=STATIC / "terminal-common.js")
        await page.evaluate(
            f"""() => {{
              window.state = {{
                activeSessionId: 'terminal-session',
                renamingSessionId: '',
              }};
              window.activeTerminalContext = {{ sessionId: 'terminal-session' }};
              window.terminalPasteDialogEl = null;
              window.terminalAgentsDocDialogEl = null;
              window.terminalInputHistoryDialogEl = null;
              window.terminalCommandCollectionsMenuEl = null;
              window.terminalQuotaDialogEl = null;
              window.terminalAgentsDocEditorEl = null;
              window.terminalAgentsDocSelectEl = null;
              window.terminalSystemKeyboardCheckboxEl = null;
              window.syncTerminalImePolicy = () => {{}};
              window.blurTerminalHelperTextarea = () => {{}};
              window.terminalHelperTextarea = () => null;
              window.terminalSoftKeyboardVisible = () => false;
              window.focusTerminalIfAllowed = () => {{}};
              window.focusTerminalAfterSoftKeyboardInput = () => {{}};
              window.updateStatus = (message, tone) => {{
                window.__copyStatus = {{ message, tone }};
              }};
              window.activeSession = () => ({{ name: {TERMINAL_NAME!r} }});
              window.readTerminalBufferTailText = () =>
                'Session ID: {SESSION_ID}';
              window.terminalResumeExtract = null;
              window.CODEX_RESUME_ID_PATTERN = /^[0-9a-f-]{{36}}$/;
              window.CODEX_RESUME_UUID_PATTERN = /^[0-9a-f-]{{36}}$/;
              window.CODEX_RESUME_COMMAND_PATTERN = /codex resume\\s+([0-9a-f-]{{36}})/gi;
              window.CLAUDE_RESUME_COMMAND_PATTERN = /claude --resume\\s+([0-9a-f-]{{36}})/gi;
              window.BANNER_SESSION_LABEL_PATTERN = /Session ID:\\s*([0-9a-f-]{{36}})/gi;
              window.CLAUDE_BANNER_SESSION_LABEL_PATTERN = /Claude Session ID:\\s*([0-9a-f-]{{36}})/gi;
            }}"""
        )
        await page.add_script_tag(path=STATIC / "terminal-focus-selection.js")
        await page.add_script_tag(path=STATIC / "terminal-resume-agent.js")
        await page.evaluate(
            f"""() => {{
              detectCurrentAgentResumeId = async () => ({{
                resumeId: {SESSION_ID!r},
                command: `codex resume {SESSION_ID}`,
              }});
              detectAgentResumeIdComplete = async () => ({{
                resumeId: {SESSION_ID!r},
                command: `codex resume {SESSION_ID}`,
              }});
              readTerminalBufferTailText = () => 'Session ID: {SESSION_ID}';
              installTerminalProgrammaticFocusGate();
              document.addEventListener('focusin', rejectTerminalActionEditableFocus, true);
              window.__clipboardText = '';
              document.addEventListener('copy', (event) => {{
                const active = document.activeElement;
                const selectedText = active instanceof HTMLTextAreaElement
                  ? active.value.slice(active.selectionStart, active.selectionEnd)
                  : window.getSelection()?.toString() || '';
                window.__clipboardText = selectedText;
                event.preventDefault();
              }});
              const terminalInput = document.createElement('textarea');
              terminalInput.id = 'test-terminal-input';
              terminalInput.value = window.__wrongSelection =
                'Already viewing the wrong terminal selection';
              document.body.appendChild(terminalInput);
              terminalInput.focus();
              terminalInput.select();
              terminalActionKeyboardPolicyActive = true;
              terminalActionEditableFocusBlocked = true;
              Object.defineProperty(navigator, 'clipboard', {{
                configurable: true,
                value: {{
                  writeText: async () => {{ throw new Error('WebView clipboard denied'); }},
                  readText: async () => window.__clipboardText,
                }},
              }});
            }}"""
        )

        async def reset_selection():
            await page.evaluate(
                """() => {
                  const terminalInput = document.getElementById('test-terminal-input');
                  terminalInput.focus();
                  terminalInput.select();
                  terminalActionKeyboardPolicyActive = true;
                  terminalActionEditableFocusBlocked = true;
                }"""
            )

        async def clipboard_text():
            return await page.evaluate("window.__clipboardText")

        await reset_selection()
        await page.evaluate("copyCurrentTerminalName()")
        assert await clipboard_text() == TERMINAL_NAME

        await reset_selection()
        await page.evaluate("extractCurrentAgentSessionId()")
        assert await clipboard_text() == SESSION_ID, await page.evaluate(
            "({ clipboard: window.__clipboardText, status: window.__copyStatus })"
        )

        await reset_selection()
        await page.evaluate("copyCurrentSessionIdAndAsk()")
        assert await clipboard_text() == (
            f"调用codex db维护skill读取session id为 {SESSION_ID}并接受指令："
        )

        # Context 缺失/不匹配时也必须先走检测链拿 ID，再拼提问语句。
        await reset_selection()
        mismatch_status = await page.evaluate(
            """async () => {
              activeTerminalContext = { sessionId: 'other-terminal-session' };
              const detectCalls = [];
              const originalDetect = detectAgentResumeIdComplete;
              detectAgentResumeIdComplete = async (sessionId, context) => {
                detectCalls.push({ sessionId, context });
                return originalDetect(sessionId, context);
              };
              try {
                await copyCurrentSessionIdAndAsk();
              } finally {
                detectAgentResumeIdComplete = originalDetect;
                activeTerminalContext = { sessionId: 'terminal-session' };
              }
              return {
                detectCalls,
                status: window.__copyStatus,
              };
            }"""
        )
        assert mismatch_status["detectCalls"] == [
            {"sessionId": "terminal-session", "context": None}
        ], mismatch_status
        assert await clipboard_text() == (
            f"调用codex db维护skill读取session id为 {SESSION_ID}并接受指令："
        )
        assert "尚未连接" not in mismatch_status["status"]["message"], mismatch_status["status"]

        await reset_selection()
        await page.evaluate("copyLatestResumeId()")
        await page.locator("#terminal-copy-resume-overlay button", has_text="复制").click()
        assert await clipboard_text() == SESSION_ID

        assert WRONG_SELECTION not in [
            await clipboard_text(),
            TERMINAL_NAME,
            SESSION_ID,
        ]
        await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
