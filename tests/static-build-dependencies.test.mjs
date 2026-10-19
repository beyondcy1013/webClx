import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

// Run through the compile API: this test invokes Cargo on a small, isolated
// fixture to exercise real incremental-build fingerprints (no worktrees).
test('embedded assets track edits, additions, removals and symlink replacement', { timeout: 60000 }, () => {
  const root = mkdtempSync(join(tmpdir(), 'webclx-static-build-'));
  try {
    mkdirSync(join(root, 'src'));
    mkdirSync(join(root, 'assets'));
    mkdirSync(join(root, 'assets', 'vendor'));
    symlinkSync(join(root, 'assets'), join(root, 'static'), 'dir');
    writeFileSync(join(root, 'assets', 'index.html'), 'first');
    writeFileSync(join(root, 'Cargo.toml'), '[package]\nname="static-fixture"\nversion="0.0.0"\nedition="2024"\n[workspace]\n');
    writeFileSync(join(root, 'src', 'main.rs'), 'fn main() {}\n');
    const build = readFileSync(new URL('../build.rs', import.meta.url), 'utf8')
      .replace('fn main()', 'fn webclx_build()');
    writeFileSync(join(root, 'build.rs'), build + `
fn main() {
    webclx_build();
    use std::io::Write;
    std::fs::OpenOptions::new().create(true).append(true)
        .open("build-runs").unwrap().write_all(b"x").unwrap();
}
`);
    const buildCount = () => {
      const result = spawnSync('cargo', ['build', '--offline', '--quiet'], {
        cwd: root, encoding: 'utf8', timeout: 20000, killSignal: 'SIGKILL',
      });
      assert.equal(result.status, 0, result.error?.message || result.stderr);
      return readFileSync(join(root, 'build-runs'), 'utf8').length;
    };
    assert.equal(buildCount(), 1);
    assert.equal(buildCount(), 1, 'unchanged assets should reuse the build');
    writeFileSync(join(root, 'assets', 'index.html'), 'changed');
    assert.equal(buildCount(), 2, 'asset content changes must invalidate the build');
    writeFileSync(join(root, 'assets', 'vendor', 'added.js'), 'added');
    assert.equal(buildCount(), 3, 'new nested assets must invalidate the build');
    rmSync(join(root, 'assets', 'vendor', 'added.js'));
    assert.equal(buildCount(), 4, 'removed assets must invalidate the build');
    mkdirSync(join(root, 'replacement'));
    writeFileSync(join(root, 'replacement', 'index.html'), 'replacement');
    rmSync(join(root, 'static'));
    symlinkSync(join(root, 'replacement'), join(root, 'static'), 'dir');
    assert.equal(buildCount(), 5, 'repointing static must invalidate the build');
    assert.equal(buildCount(), 5);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
