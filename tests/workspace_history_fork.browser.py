import json
import os

from playwright.sync_api import sync_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome",
)


def main() -> None:
    console_errors = []
    page_errors = []
    failed_responses = []
    preset_apply_requests = []
    results = {"viewports": {}}

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "response",
            lambda response: failed_responses.append(
                {"status": response.status, "url": response.url}
            )
            if response.url.startswith(BASE_URL) and response.status >= 400
            else None,
        )

        page.goto(f"{BASE_URL}/workspace_history", wait_until="domcontentloaded")
        page.wait_for_function(
            "() => typeof renderWorkspaceHistory === 'function'"
            " && typeof openFreshTerminalRunLink === 'function'"
        )
        page.wait_for_function(
            "() => state.workspaceHistoryLoadState === 'loaded'",
            timeout=30_000,
        )
        page.evaluate(
            """() => {
                const cwd = '/home/codes/webClx';
                const codexId = '019f2350-db5f-7cf0-b476-1cf14855b05d';
                const claudeId = '019f2350-db5f-7cf0-b476-1cf14855b05e';
                const now = Date.now();
                window.__workspaceHistoryForkCalls = [];
                window.__workspaceHistoryPresetForkCalls = [];
                openFreshTerminalRunLink = (event, path, command, options = {}) => {
                    event?.preventDefault?.();
                    window.__workspaceHistoryForkCalls.push({
                        path,
                        command,
                        terminalName: options.terminalName || '',
                    });
                };
                openFreshTerminalSession = (path, options = {}) => {
                    window.__workspaceHistoryPresetForkCalls.push({
                        path,
                        command: options.runCommand || '',
                        terminalName: options.terminalName || '',
                        quickStart: options.quickStart,
                    });
                };

                state.workspaceDir = cwd;
                state.workspaceHistory = [{ path: cwd, last_opened_at: now }];
                state.workspaceHistorySelectedPath = cwd;
                state.workspaceHistorySearchAllWorkspaces = false;
                state.workspaceHistorySearchQuery = '';
                state.workspaceHistoryRecentOnly = false;
                state.workspaceHistoryLoadState = 'loaded';
                state.sessions = [];
                state.codexConversations = [
                    {
                        session_id: codexId,
                        cwd,
                        title: 'QA Codex history',
                        updated_at: now,
                        size_bytes: 1024,
                    },
                ];
                state.terminalArchives = [
                    {
                        resume_id: codexId,
                        cwd,
                        terminal_name: 'webClx_QA',
                        command: `codex resume ${codexId}`,
                        note: 'QA Codex history',
                        updated_at: now,
                    },
                    {
                        resume_id: claudeId,
                        cwd,
                        terminal_name: 'claude_QA',
                        command: `claude --resume ${claudeId}`,
                        note: 'QA Claude history',
                        updated_at: now - 1,
                    },
                ];
                renderWorkspaceHistory();
            }"""
        )

        rows = page.locator("#workspace-history-list tr")
        assert rows.count() == 2
        codex_row = rows.filter(has_text="QA Codex history")
        claude_row = rows.filter(has_text="QA Claude history")
        assert codex_row.count() == 1
        assert claude_row.count() == 1
        assert codex_row.get_by_role("link", name="fork", exact=True).count() == 1
        assert claude_row.get_by_role("link", name="fork", exact=True).count() == 0
        assert codex_row.locator(".session-action-cell .mini-button").all_inner_texts() == [
            "恢复",
            "fork",
            "模型",
            "更多",
        ]

        page.route(
            "**/api/auth/api-presets",
            lambda route: route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps(
                    {
                        "presets": [
                            {
                                "id": "api-current",
                                "name": "Current Sol",
                                "base_url": "https://current.example/v1",
                                "active": True,
                                "config_overrides": [
                                    {"key": "model", "value": "gpt-5.6-sol"}
                                ],
                            },
                            {
                                "id": "api-backup",
                                "name": "Backup Terra",
                                "base_url": "https://backup.example/v1",
                                "active": False,
                                "config_overrides": [
                                    {"key": "model", "value": "gpt-5.6-terra"}
                                ],
                            },
                        ]
                    }
                ),
            ),
        )
        def fulfill_preset_apply(route) -> None:
            preset_apply_requests.append(
                {"method": route.request.method, "url": route.request.url}
            )
            route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps({"id": "api-backup", "name": "Backup Terra"}),
            )

        page.route(
            "**/api/auth/api-presets/api-backup/apply",
            fulfill_preset_apply,
        )

        codex_row.get_by_role("button", name="指定大模型", exact=True).click()
        preset_dialog = page.locator("#workspace-history-preset-dialog")
        assert preset_dialog.is_visible()
        page.wait_for_function(
            "() => document.querySelectorAll('#workspace-history-preset-list .workspace-history-preset-option').length === 2"
        )
        assert preset_dialog.locator(".workspace-history-preset-option").count() == 2
        assert preset_dialog.get_by_text("gpt-5.6-sol", exact=False).count() == 1
        assert preset_dialog.get_by_text("gpt-5.6-terra", exact=False).count() == 1
        page.wait_for_function(
            "() => !document.querySelector('#workspace-history-preset-submit')?.disabled"
        )
        assert preset_dialog.get_by_role(
            "button", name="以此预设 fork", exact=True
        ).is_enabled()

        results["dialogViewports"] = {}
        for width, height in ((375, 812), (1440, 1000)):
            page.set_viewport_size({"width": width, "height": height})
            dialog_geometry = preset_dialog.evaluate(
                """dialog => {
                    const rect = dialog.getBoundingClientRect();
                    const submit = dialog.querySelector('#workspace-history-preset-submit')
                        ?.getBoundingClientRect();
                    return {
                        rect: {
                            left: rect.left,
                            right: rect.right,
                            top: rect.top,
                            bottom: rect.bottom,
                            width: rect.width,
                            height: rect.height,
                        },
                        contained: rect.left >= 0
                            && rect.right <= window.innerWidth
                            && rect.top >= 0
                            && rect.bottom <= window.innerHeight,
                        submitVisible: Boolean(
                            submit
                            && submit.width > 0
                            && submit.height > 0
                            && submit.left >= rect.left
                            && submit.right <= rect.right
                            && submit.top >= rect.top
                            && submit.bottom <= rect.bottom
                        ),
                        submitDisabled: Boolean(
                            dialog.querySelector('#workspace-history-preset-submit')?.disabled
                        ),
                        pageOverflow: document.documentElement.scrollWidth
                            > document.documentElement.clientWidth,
                    };
                }"""
            )
            assert dialog_geometry["contained"]
            assert dialog_geometry["submitVisible"]
            assert not dialog_geometry["submitDisabled"]
            assert not dialog_geometry["pageOverflow"]
            results["dialogViewports"][str(width)] = dialog_geometry
            page.screenshot(
                path=f"/tmp/webclx-workspace-history-preset-dialog-{width}.png",
                full_page=True,
            )

        page.set_viewport_size({"width": 1440, "height": 1000})
        preset_dialog.get_by_text("Backup Terra", exact=True).click()
        preset_dialog.get_by_role("button", name="以此预设 fork", exact=True).click()
        page.wait_for_function(
            "() => window.__workspaceHistoryPresetForkCalls.length === 1"
        )
        results["presetClick"] = page.evaluate("window.__workspaceHistoryPresetForkCalls")
        results["presetApplyRequests"] = preset_apply_requests
        assert results["presetApplyRequests"] == [
            {
                "method": "PUT",
                "url": f"{BASE_URL}/api/auth/api-presets/api-backup/apply",
            }
        ]
        assert results["presetClick"] == [
            {
                "path": "/home/codes/webClx",
                "command": "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
                "terminalName": "webClx_QA_fork",
                "quickStart": False,
            }
        ]
        assert not preset_dialog.is_visible()

        codex_row.get_by_role("link", name="fork", exact=True).click()
        results["click"] = page.evaluate("window.__workspaceHistoryForkCalls")
        assert results["click"] == [
            {
                "path": "/home/codes/webClx",
                "command": "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
                "terminalName": "webClx_QA_fork",
            }
        ]

        for width, height in ((375, 812), (768, 900), (1440, 1000)):
            page.set_viewport_size({"width": width, "height": height})
            geometry = codex_row.locator(".session-action-cell").evaluate(
                """cell => {
                    const cellRect = cell.getBoundingClientRect();
                    const buttons = Array.from(cell.querySelectorAll('.mini-button')).map(
                        (button) => {
                            const rect = button.getBoundingClientRect();
                            return {
                                left: rect.left,
                                right: rect.right,
                                top: rect.top,
                                bottom: rect.bottom,
                                width: rect.width,
                                height: rect.height,
                            };
                        },
                    );
                    return {
                        buttons,
                        contained: buttons.every(
                            (rect) => rect.left >= cellRect.left - 1
                                && rect.right <= cellRect.right + 1
                                && rect.top >= cellRect.top - 1
                                && rect.bottom <= cellRect.bottom + 1,
                        ),
                        separated: buttons.every(
                            (rect, index) => index === 0
                                || rect.left >= buttons[index - 1].right - 0.5,
                        ),
                        pageOverflow: document.documentElement.scrollWidth
                            > document.documentElement.clientWidth,
                    };
                }"""
            )
            assert geometry["contained"]
            assert geometry["separated"]
            assert all(
                button["width"] > 0 and button["height"] > 0
                for button in geometry["buttons"]
            )
            assert not geometry["pageOverflow"]
            results["viewports"][str(width)] = geometry
            page.screenshot(
                path=f"/tmp/webclx-workspace-history-fork-{width}.png",
                full_page=True,
            )

        assert console_errors == []
        assert page_errors == []
        assert failed_responses == []
        results["consoleErrors"] = console_errors
        results["pageErrors"] = page_errors
        results["failedResponses"] = failed_responses
        browser.close()

    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
