"""Exercise runtime layout code with real xterm, without a persistent PTY."""
import asyncio
from pathlib import Path
from playwright.async_api import async_playwright

STATIC = Path('/home/bin/webclx/static')

async def main():
    async with async_playwright() as pw:
        browser = await pw.chromium.launch(
            executable_path='/home/third_party/browser-tools/bin/chromium',
            args=['--no-sandbox'],
        )
        try:
            for mobile in (False, True):
                context = await browser.new_context(
                    viewport={'width': 390 if mobile else 1440, 'height': 844},
                    is_mobile=mobile, has_touch=mobile,
                )
                page = await context.new_page()
                await page.set_content('''<meta name="viewport" content="width=device-width, initial-scale=1">
                    <style>body{margin:0} #shell{width:100%}
                    #host{width:100%;height:600px;box-sizing:border-box;padding:4px}</style>
                    <div id="shell" class="terminal-scroll-shell"><div id="host"></div></div>''')
                await page.add_style_tag(path=str(STATIC / 'vendor/xterm.css'))
                await page.add_style_tag(path=str(STATIC / 'styles-terminal.css'))
                for script in ('vendor/xterm.js', 'vendor/xterm-addon-fit.js',
                               'terminal-shell-settings.js', 'terminal-layout-connection.js'):
                    await page.add_script_tag(path=str(STATIC / script))
                await page.evaluate('''async () => {
                    window.terminalHost = document.querySelector('#host');
                    window.terminalScrollShellEl = document.querySelector('#shell');
                    window.state = {terminalWideMode: false};
                    const term = new Terminal({cols:201, rows:52, fontSize:14});
                    const fitAddon = new FitAddon.FitAddon();
                    term.loadAddon(fitAddon); term.open(terminalHost);
                    activeTerminalContext = {term, fitAddon,
                        canonicalTerminalSize:{cols:201,rows:52}};
                    await new Promise(resolve => term.write('HEADER\\x1b[3;150HEND', resolve));
                    activeTerminalContext.localTerminalFitSize = terminalLocalFitSize();
                    window.originalFit = terminalLocalFitSize();
                    applyTerminalCanonicalViewportLayout();
                }''')
                geometry = await page.evaluate('''() => ({
                    width:terminalHost.getBoundingClientRect().width,
                    available:terminalScrollShellEl.clientWidth,
                    scale:Number(terminalHost.style.zoom || 1),
                    fit:terminalLocalFitSize(), original:originalFit,
                    cols:activeTerminalContext.term.cols,
                    end:activeTerminalContext.term.buffer.active.getLine(2).translateToString(true)
                })''')
                assert geometry['fit'] == geometry['original'], geometry
                assert geometry['cols'] == 201 and geometry['end'].endswith('END'), geometry
                if mobile:
                    assert geometry['scale'] == 1, geometry
                    assert geometry['width'] > geometry['available'], geometry
                    assert await page.evaluate('''() => {
                        terminalScrollShellEl.scrollLeft = 100;
                        return terminalScrollShellEl.scrollLeft === 100 &&
                            getComputedStyle(terminalScrollShellEl).overflowX === 'auto';
                    }''')
                    # Rotation preserves native font size and horizontal scrolling.
                    await page.set_viewport_size({'width': 844, 'height': 390})
                    await page.evaluate('''() => {
                        activeTerminalContext.localTerminalFitSize=terminalLocalFitSize();
                        applyTerminalCanonicalViewportLayout();
                    }''')
                    assert await page.evaluate('Number(terminalHost.style.zoom || 1) === 1 && terminalHost.getBoundingClientRect().width > terminalScrollShellEl.clientWidth')
                else:
                    assert geometry['scale'] == 1, geometry
                # When desktop leaves, the canonical grid fits the local client again.
                await page.evaluate('''() => {
                    activeTerminalContext.canonicalTerminalSize=terminalLocalFitSize();
                    activeTerminalContext.localTerminalFitSize=terminalLocalFitSize();
                    applyTerminalCanonicalViewportLayout();
                }''')
                assert await page.evaluate('terminalHost.style.zoom === "" && terminalHost.style.width === ""')
                print(('mobile' if mobile else 'desktop') + ': geometry, ANSI grid, native fit and reset passed')
                await context.close()
        finally:
            await browser.close()

if __name__ == '__main__':
    asyncio.run(asyncio.wait_for(main(), timeout=45))
