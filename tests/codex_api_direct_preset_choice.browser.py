import json
import mimetypes
import os
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
BASE_URL = "http://127.0.0.1:11111"
PRESET_ID = "api-direct-minimax-test"
USE_LOCAL_STATIC = os.environ.get("WEBCLX_PRESET_UI_LIVE") != "1"


def direct_minimax_preset():
    return {
        "id": PRESET_ID,
        "name": "Direct MiniMax regression",
        "provider_name": "MiniMax",
        "base_url": "https://api.minimaxi.com/v1",
        "management_url": "https://api.minimaxi.com/v1",
        "wire_api": "responses",
        "responses_proxy": None,
        "apply_upstream_proxy_on_switch": False,
        "config_overrides": [
            {"key": "model", "value": "MiniMax-M3"},
            {"key": "model_reasoning_effort", "value": "xhigh"},
        ],
        "api_key": "sk-direct-minimax-test",
        "masked_api_key": "sk-dir***test",
        "access_mode": "direct",
        "saved_at": 1_785_000_000,
        "active": False,
        "switch_count": 0,
    }


def route_test_page(route, request, captured_payloads, preset_state):
    parsed = urlparse(request.url)
    if parsed.hostname not in {"127.0.0.1", "localhost"}:
        route.fulfill(status=204, content_type="text/plain", body=b"")
        return

    if parsed.path in {"/", "/codex_api"}:
        if USE_LOCAL_STATIC:
            route.fulfill(
                status=200,
                content_type="text/html; charset=utf-8",
                body=(STATIC / "index.html").read_bytes(),
            )
        else:
            route.continue_()
        return

    if parsed.path.startswith("/assets/"):
        if USE_LOCAL_STATIC:
            candidate = (STATIC / parsed.path.removeprefix("/assets/")).resolve()
            if candidate.is_relative_to(STATIC) and candidate.is_file():
                content_type = mimetypes.guess_type(candidate.name)[0] or "application/octet-stream"
                route.fulfill(status=200, content_type=content_type, body=candidate.read_bytes())
                return
        route.continue_()
        return

    if parsed.path == "/api/auth/api-presets":
        route.fulfill(
            status=200,
            content_type="application/json",
            body=json.dumps(
                {
                    "auth_file": "/home/root/.codex/auth.json",
                    "config_file": "/home/root/.codex/config.toml",
                    "preset_file": "/tmp/webclx-api-presets.json",
                    "current_mode": "api",
                    "current_api": None,
                    "current_auth_error": None,
                    "current_config_error": None,
                    "upstream_proxy": {
                        "codex_api_proxy_enabled": False,
                        "claude_proxy_enabled": False,
                        "active_api_proxy_preset_id": None,
                        "active_claude_proxy_preset_id": None,
                    },
                    "presets": [preset_state],
                }
            ),
        )
        return

    if parsed.path == "/api/auth/api-presets/reorder" and request.method == "PUT":
        route.fulfill(
            status=200,
            content_type="application/json",
            body=json.dumps({"ok": True, "presets": [preset_state]}),
        )
        return

    if parsed.path == f"/api/auth/api-presets/{PRESET_ID}" and request.method == "PUT":
        payload = request.post_data_json
        captured_payloads.append(payload)
        preset_state.update(payload)
        route.fulfill(
            status=200,
            content_type="application/json",
            body=json.dumps({"ok": True, "preset": preset_state}),
        )
        return

    route.continue_()


def main():
    captured_payloads = []
    preset_state = direct_minimax_preset()
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        page.route(
            "**/*",
            lambda route, request: route_test_page(
                route, request, captured_payloads, preset_state
            ),
        )
        page.goto(f"{BASE_URL}/codex_api", wait_until="domcontentloaded", timeout=30_000)

        row = page.locator("#api-preset-list tr", has_text="Direct MiniMax regression")
        row.wait_for(timeout=10_000)
        row.locator(".preset-action-menu-trigger").click()
        page.locator(".preset-action-menu").get_by_role(
            "menuitem", name="编辑", exact=True
        ).click()

        page.locator("#api-preset-editor-panel").wait_for(state="visible")
        assert page.locator("#api-responses-proxy-input").input_value() == ""
        assert not page.locator("#api-apply-upstream-proxy-on-switch").is_checked()
        effort_index = page.locator(".config-override-key-input").evaluate_all(
            "inputs => inputs.findIndex(input => input.value === 'model_reasoning_effort')"
        )
        assert effort_index >= 0
        effort_row = page.locator(".config-override-row").nth(effort_index)
        effort_row.locator(".config-override-value-input").fill("medium")

        # Simulate another list refresh already being in flight when the save
        # completes. The saved PUT response must still update the editor state.
        page.evaluate("state.apiPresetsLoading = true")
        page.on("dialog", lambda dialog: dialog.accept())
        page.locator("#api-save-preset").click()
        page.wait_for_function("() => document.querySelector('#api-preset-editor-panel').hidden")
        page.evaluate("state.apiPresetsLoading = false")

        assert len(captured_payloads) == 1
        assert captured_payloads[0]["responses_proxy"] is None
        assert captured_payloads[0]["apply_upstream_proxy_on_switch"] is False
        assert {
            "key": "model_reasoning_effort",
            "value": "medium",
        } in captured_payloads[0]["config_overrides"]

        row = page.locator("#api-preset-list tr", has_text="Direct MiniMax regression")
        row.locator(".preset-action-menu-trigger").click()
        page.locator(".preset-action-menu").get_by_role(
            "menuitem", name="编辑", exact=True
        ).click()
        page.locator("#api-preset-editor-panel").wait_for(state="visible")
        effort_index = page.locator(".config-override-key-input").evaluate_all(
            "inputs => inputs.findIndex(input => input.value === 'model_reasoning_effort')"
        )
        assert effort_index >= 0
        effort_row = page.locator(".config-override-row").nth(effort_index)
        assert effort_row.locator(".config-override-value-input").input_value() == "medium"
        browser.close()

    print(
        json.dumps(
            {
                "responsesProxy": captured_payloads[0]["responses_proxy"],
                "applyLocalEntry": captured_payloads[0][
                    "apply_upstream_proxy_on_switch"
                ],
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()
