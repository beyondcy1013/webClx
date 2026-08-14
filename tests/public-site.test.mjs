import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const html = readFileSync(new URL('../site/index.html', import.meta.url), 'utf8');
const css = readFileSync(new URL('../site/styles.css', import.meta.url), 'utf8');
const script = readFileSync(new URL('../site/site.js', import.meta.url), 'utf8');
const workflow = readFileSync(new URL('../.github/workflows/pages.yml', import.meta.url), 'utf8');

test('public site exposes download, trial, security, and commercial paths', () => {
  assert.match(html, /releases\/tag\/v1\.8\.11/);
  assert.match(html, /hosted-trial\.yml/);
  assert.match(html, /COMMERCIAL\.md/);
  assert.match(html, /SECURITY\.md/);
});

test('public site is bilingual and labels hosted access as invite-only', () => {
  assert.match(html, /data-en=/);
  assert.match(html, /data-zh=/);
  assert.match(html, /Invite-only/);
  assert.match(html, /仅限邀请/);
  assert.match(script, /localStorage\.setItem/);
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

test('GitHub Pages workflow deploys only the public site directory', () => {
  assert.match(workflow, /actions\/upload-pages-artifact@v4/);
  assert.match(workflow, /path: site/);
  assert.match(workflow, /actions\/deploy-pages@v4/);
  assert.doesNotMatch(workflow, /fpsq\.xyz|\/home\//);
});
