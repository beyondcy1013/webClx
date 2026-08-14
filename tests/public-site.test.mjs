import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const html = readFileSync(new URL('../site/index.html', import.meta.url), 'utf8');
const css = readFileSync(new URL('../site/styles.css', import.meta.url), 'utf8');
const script = readFileSync(new URL('../site/site.js', import.meta.url), 'utf8');
const demoHtml = readFileSync(new URL('../site/demo.html', import.meta.url), 'utf8');
const demoCss = readFileSync(new URL('../site/demo.css', import.meta.url), 'utf8');
const demoScript = readFileSync(new URL('../site/demo.js', import.meta.url), 'utf8');
const workflow = readFileSync(new URL('../.github/workflows/pages.yml', import.meta.url), 'utf8');
const robots = readFileSync(new URL('../site/robots.txt', import.meta.url), 'utf8');
const sitemap = readFileSync(new URL('../site/sitemap.xml', import.meta.url), 'utf8');

test('public site exposes download, trial, security, and commercial paths', () => {
  assert.match(html, /releases\/tag\/v1\.8\.11/);
  assert.match(html, /hosted-trial\.yml/);
  assert.match(html, /COMMERCIAL\.md/);
  assert.match(html, /SECURITY\.md/);
  assert.match(html, /href="demo\.html"/);
});

test('interactive demo is browser-only, synthetic, and covers the full workflow', () => {
  assert.match(demoHtml, /NO ACCOUNT/);
  assert.match(demoHtml, /SYNTHETIC SESSION/);
  assert.match(demoHtml, /data-action="disconnect"/);
  assert.match(demoHtml, /data-action="resume"/);
  assert.match(demoHtml, /data-action="review"/);
  assert.match(demoHtml, /data-action="reply"/);
  assert.match(demoScript, /demo\.step = 5/);
  assert.doesNotMatch(`${demoHtml}\n${demoScript}`, /fetch\(|WebSocket|\/api\//);
  assert.doesNotMatch(`${demoHtml}\n${demoScript}`, /\/home\/(codes|root|bin)|fpsq\.xyz|password|api[_-]?key/i);
});

test('interactive demo supports mobile layout and reduced motion', () => {
  assert.match(demoCss, /view-mobile/);
  assert.match(demoCss, /@media\(max-width:620px\)/);
  assert.match(demoCss, /prefers-reduced-motion:reduce/);
});

test('public site is bilingual and labels hosted access as invite-only', () => {
  assert.match(html, /data-en=/);
  assert.match(html, /data-zh=/);
  assert.match(html, /Invite-only/);
  assert.match(html, /仅限邀请/);
  assert.match(script, /localStorage\.setItem/);
});

test('public positioning leads with the verified away-from-desk workflow', () => {
  assert.match(html, /Monitor, approve, or steer the same live CLI from your phone/);
  assert.match(html, /查看进度、批准操作或发送简短指令/);
  assert.match(html, /Return to the same live session/);
  assert.match(html, /回到同一个实时会话/);
  assert.doesNotMatch(html, /code comfortably on your phone|replace your desktop/i);
});

test('public site uses the synthetic demonstration and avoids internal paths', () => {
  assert.match(html, /assets\/webclx-remote-workflow\.png/);
  assert.doesNotMatch(html, /\/home\/(codes|root|bin)|fpsq\.xyz:11112|api[_-]?key|password/i);
});

test('public site defines responsive and reduced-motion behavior', () => {
  assert.match(css, /@media\(max-width:620px\)/);
  assert.match(css, /prefers-reduced-motion:reduce/);
  assert.match(css, /min-height:calc\(100dvh/);
});

test('public site exposes crawler and canonical sharing metadata', () => {
  assert.match(html, /<link rel="canonical" href="https:\/\/beyondcy1013\.github\.io\/webClx\/">/);
  assert.match(html, /application\/ld\+json/);
  assert.match(robots, /User-agent: \*/);
  assert.match(robots, /Sitemap: https:\/\/beyondcy1013\.github\.io\/webClx\/sitemap\.xml/);
  assert.match(sitemap, /https:\/\/beyondcy1013\.github\.io\/webClx\//);
  assert.match(sitemap, /https:\/\/beyondcy1013\.github\.io\/webClx\/demo\.html/);
  assert.match(sitemap, /https:\/\/github\.com\/beyondcy1013\/webClx\/releases\/tag\/v1\.8\.11/);
});

test('GitHub Pages workflow deploys only the public site directory', () => {
  assert.match(workflow, /actions\/upload-pages-artifact@v4/);
  assert.match(workflow, /path: site/);
  assert.match(workflow, /actions\/deploy-pages@v4/);
  assert.doesNotMatch(workflow, /fpsq\.xyz|\/home\//);
});
