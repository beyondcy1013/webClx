from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]


def test_workspace_icon_select_interaction() -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 375, "height": 720})
        page.route(
            "http://icons.test/**",
            lambda route: route.fulfill(
                status=200,
                content_type="image/svg+xml",
                body='<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24">'
                '<rect width="24" height="24" fill="#287b62"/></svg>',
            ),
        )
        page.set_content(
            """
            <base href="http://icons.test/">
            <select id="sessions" aria-label="切换当前终端">
              <option value="root" data-workspace-path="">工作区根目录</option>
              <option value="alpha" data-workspace-path="alpha" selected>Alpha 终端</option>
              <option value="beta" data-workspace-path="beta/src">Beta 终端</option>
            </select>
            """
        )
        page.add_style_tag(path=ROOT / "static/styles-base.css")
        page.add_script_tag(path=ROOT / "static/workspace-project-icons.js")
        page.evaluate(
            """
            window.changeCount = 0;
            const select = document.querySelector('#sessions');
            select.addEventListener('change', () => window.changeCount += 1);
            WebClxWorkspaceProjectIcons.enhanceWorkspaceIconSelect(
              select,
              () => 'static/favicon.svg',
            );
            """
        )

        trigger = page.get_by_role("button", name="切换当前终端")
        trigger.click()
        menu = page.get_by_role("listbox")
        assert menu.is_visible()
        assert page.get_by_role("option").count() == 3
        assert page.get_by_role("option", name="工作区根目录").locator(
            ".workspace-project-icon"
        ).count() == 1
        beta_option = page.get_by_role("option", name="Beta 终端")
        beta_option.hover()
        assert beta_option.evaluate(
            "element => getComputedStyle(element).backgroundColor"
        ) != "rgba(0, 0, 0, 0)"
        beta_option.click()
        assert page.locator("#sessions").input_value() == "beta"
        assert page.evaluate("window.changeCount") == 1
        assert "Beta 终端" in trigger.inner_text()

        trigger.click()
        page.keyboard.press("Escape")
        assert menu.is_hidden()
        assert page.evaluate("document.activeElement === document.querySelector('.workspace-icon-select-trigger')")
        visible_icon = page.locator(".workspace-project-icon:not([hidden])").first
        visible_icon.wait_for(state="visible", timeout=5_000)
        assert visible_icon.is_visible()
        assert page.evaluate("document.documentElement.scrollWidth <= document.documentElement.clientWidth")

        browser.close()


def test_workspace_icon_select_escapes_scrolling_toolbar() -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 375, "height": 720})
        page.route(
            "http://icons.test/**",
            lambda route: route.fulfill(status=404, content_type="text/plain", body="missing"),
        )
        page.set_content(
            """
            <base href="http://icons.test/">
            <header class="topbar slim compact terminal-control-bar">
              <label class="terminal-session-picker" for="sessions">
                <select id="sessions" aria-label="切换当前终端">
                  <option value="alpha" data-workspace-path="alpha" selected>Alpha 终端</option>
                  <option value="alpha-src" data-workspace-path="alpha/src">Alpha 子目录终端</option>
                  <option value="beta" data-workspace-path="beta">Beta 终端</option>
                </select>
              </label>
            </header>
            """
        )
        page.add_style_tag(path=ROOT / "static/styles-base.css")
        page.add_script_tag(path=ROOT / "static/workspace-project-icons.js")
        page.evaluate(
            """
            WebClxWorkspaceProjectIcons.enhanceWorkspaceIconSelect(
              document.querySelector('#sessions'),
              () => 'static/favicon.svg',
            );
            """
        )

        trigger = page.get_by_role("button", name="切换当前终端")
        trigger.click()
        page.wait_for_function(
            "() => document.querySelectorAll("
            "'.workspace-icon-select-menu .workspace-project-icon-image[hidden]'"
            ").length === 3"
        )
        fallback_icons = page.locator(
            ".workspace-icon-select-menu .workspace-project-text-icon"
        ).all_inner_texts()
        assert fallback_icons[0] == fallback_icons[1]
        assert fallback_icons[0] != fallback_icons[2]
        fallback_hues = page.locator(
            ".workspace-icon-select-menu .workspace-project-icon"
        ).evaluate_all(
            "elements => elements.map(element => "
            "element.style.getPropertyValue('--workspace-project-icon-hue'))"
        )
        assert fallback_hues[0] == fallback_hues[1] == "210"
        assert fallback_hues[2] == "347.508"
        geometry = page.evaluate(
            """
            () => {
              const toolbar = document.querySelector('.terminal-control-bar');
              const menu = document.querySelector('.workspace-icon-select-menu');
              const toolbarRect = toolbar.getBoundingClientRect();
              const menuRect = menu.getBoundingClientRect();
              const probeX = Math.min(innerWidth - 1, menuRect.left + 12);
              const probeY = Math.min(innerHeight - 1, toolbarRect.bottom + 12);
              const hit = document.elementFromPoint(probeX, probeY);
              return {
                expanded: document.querySelector('.workspace-icon-select-trigger')
                  .getAttribute('aria-expanded'),
                menuHidden: menu.hidden,
                menuBelowToolbar: menuRect.bottom > toolbarRect.bottom,
                optionVisibleBelowToolbar: Boolean(hit?.closest('.workspace-icon-select-menu')),
                menuOpaque: !getComputedStyle(menu).backgroundColor.startsWith('rgba('),
              };
            }
            """
        )
        assert geometry == {
            "expanded": "true",
            "menuHidden": False,
            "menuBelowToolbar": True,
            "optionVisibleBelowToolbar": True,
            "menuOpaque": True,
        }

        browser.close()


if __name__ == "__main__":
    test_workspace_icon_select_interaction()
    test_workspace_icon_select_escapes_scrolling_toolbar()
    print("workspace project icon browser test passed")
