from pathlib import Path
from shutil import which

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    errors: list[str] = []
    terminal = (ROOT / "static/terminal.js").read_text()
    compact_controls = terminal[
        terminal.index("function syncAutoCompactToggleForActiveSession()"):
        terminal.index("async function persistTerminalAutoContinueOnError(")
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=which("chromium") or which("google-chrome"),
            headless=True, args=["--no-sandbox"]
        )
        try:
            for width in [375, 1440]:
                page = browser.new_page(viewport={"width": width, "height": 800})
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.set_content(
                    '<label><input id="session-auto-compact-toggle" type="checkbox">自动压缩</label>'
                    '<button id="compact">/compact</button><p id="status"></p>'
                )
                page.add_script_tag(content="""
                    const state = {
                      activeSessionId: "fixture", sessions: [],
                      autoCompactSessionIds: new Set(["fixture"]),
                      autoCompactPreviousActivityStates: new Map(),
                      autoCompactStatusSessionIds: new Set(),
                      autoCompactHandledSessions: new Set(),
                      autoCompactScheduledTimers: new Map(),
                      autoCompactLastSent: new Map(), terminalUserInputAt: new Map(),
                    };
                    const sessionAutoCompactToggleEl = document.querySelector("input");
                    const inputs = [];
                    let mobileKeySendQueue = Promise.resolve();
                    const MOBILE_KEY_SEQUENCES = { enter: "\\r" };
                    const MOBILE_SLASH_COMMAND_ENTER_DELAY_MS = 1;
                    const MOBILE_SLASH_COMMAND_CONFIRM_DELAY_MS = 1;
                    function updateStatus(message) { document.querySelector("#status").textContent = message; }
                    function focusTerminalAfterSoftKeyboardInput() {}
                    function sendTerminalInput(data) { inputs.push(data); }
                    function sendTerminalInputToSession(data) { inputs.push(data); return true; }
                    function storeTerminalAutoCompactSessionIds() {}
                    function storeTerminalAutoCompactLastSent() {}
                    function storeTerminalAutoCompactStatusSessionIds() {}
                """)
                page.add_script_tag(path=str(ROOT / "static/terminal-auto-continue.js"))
                page.add_script_tag(path=str(ROOT / "static/terminal-mobile-keys.js"))
                page.add_script_tag(content=compact_controls)
                page.evaluate("""() => {
                    document.querySelector("#compact").onclick = () => sendSlashCommand("/compact");
                }""")
                for agent in ["CodeBuddy", "AGY", "Cline"]:
                    page.evaluate("""agent => {
                        state.sessions = [{ id: "fixture", name: "fixture", activity_agent: agent }];
                        syncAutoCompactToggleForActiveSession();
                    }""", agent)
                    assert page.locator("input").is_disabled()
                    assert not page.locator("input").is_checked()
                    page.locator("#compact").click()
                    assert agent in page.locator("#status").inner_text()
                    assert page.evaluate("inputs.length") == 0
                page.evaluate("""() => {
                    state.sessions = [{ id: "fixture", activity_agent: "Codex" }];
                    syncAutoCompactToggleForActiveSession();
                }""")
                assert page.locator("input").is_enabled()
                assert page.locator("input").is_checked()
                page.locator("#compact").click()
                page.evaluate("async () => { await mobileKeySendQueue; }")
                assert page.evaluate("inputs") == ["/compact", "\r", "\r"]
                page.close()
        finally:
            browser.close()
    assert not errors, errors
    print("compact agent guards passed on mobile and desktop without real terminal IO")


if __name__ == "__main__":
    main()
