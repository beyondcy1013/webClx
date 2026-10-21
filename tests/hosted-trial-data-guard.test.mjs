import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  appendFileSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const script = new URL('../scripts/hosted-trial-data-guard.sh', import.meta.url).pathname;

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'webclx-data-guard-'));
  const instance = join(root, 'instances', 'qa-demo-01');
  const workspace = join(instance, 'workspace');
  const artifacts = join(instance, 'artifacts');
  const backupRoot = join(root, 'backups');
  mkdirSync(workspace, { recursive: true });
  mkdirSync(artifacts, { recursive: true });
  writeFileSync(join(workspace, 'source.txt'), 'synthetic workspace\n');
  writeFileSync(join(artifacts, 'build.log'), 'synthetic artifact\n');
  writeFileSync(join(instance, 'manifest.env'), [
    'customer_id=qa-demo-01',
    'os_user=webclx_qa_demo_01',
    'service_name=webclx-qa-qa-demo-01.service',
    'port=12101',
    `instance_dir=${instance}`,
    `workspace_dir=${workspace}`,
    `artifact_dir=${artifacts}`,
    'state=provisioned',
  ].join('\n'));
  return { root, instance, workspace, artifacts, backupRoot };
}

test('data guard rejects non-QA identifiers and unsafe roots', () => {
  const customer = spawnSync('bash', [script, 'measure', '--customer-id', 'customer-01'], { encoding: 'utf8' });
  assert.equal(customer.status, 2);
  assert.match(customer.stderr, /qa-/i);

  const traversal = spawnSync('bash', [
    script, 'measure', '--customer-id', 'qa-demo-01', '--root-dir', '/srv/trials/../escape',
  ], { encoding: 'utf8' });
  assert.equal(traversal.status, 2);
  assert.match(traversal.stderr, /path|root|directory/i);
});

test('measure reports structured byte usage without secrets', () => {
  const item = fixture();
  try {
    const result = spawnSync('bash', [
      script, 'measure', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'), '--apply',
    ], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const body = JSON.parse(result.stdout);
    assert.equal(body.customer_id, 'qa-demo-01');
    assert.ok(body.workspace_bytes > 0);
    assert.ok(body.artifact_bytes > 0);
    assert.equal(body.total_bytes, body.workspace_bytes + body.artifact_bytes);
    assert.doesNotMatch(result.stdout, /password|token|secret/i);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('enforce dry-run stops service and freezes workspace only after an exceeded limit', () => {
  const item = fixture();
  try {
    const result = spawnSync('bash', [
      script, 'enforce', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--max-mib', '0', '--dry-run',
    ], { encoding: 'utf8' });
    assert.equal(result.status, 3, result.stderr);
    assert.match(result.stdout, /systemctl stop webclx-qa-qa-demo-01\.service/);
    assert.match(result.stdout, /chmod -R a-w/);
    assert.doesNotMatch(result.stdout, /password|token|secret/i);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('enforce leaves an under-limit instance running', () => {
  const item = fixture();
  try {
    const result = spawnSync('bash', [
      script, 'enforce', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--max-mib', '1', '--dry-run',
    ], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const body = JSON.parse(result.stdout);
    assert.equal(body.exceeded, false);
    assert.doesNotMatch(result.stdout, /systemctl|chmod/);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('backup refuses workspace symbolic links before invoking encryption', () => {
  const item = fixture();
  symlinkSync('/etc/passwd', join(item.workspace, 'escape'));
  try {
    const result = spawnSync('bash', [
      script, 'backup', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--backup-dir', item.backupRoot, '--recipient', 'fixture', '--apply',
    ], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /symbolic link/i);
    assert.equal(existsSync(item.backupRoot), false);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('backup refuses a symbolic-link backup directory', () => {
  const item = fixture();
  const outside = join(item.root, 'outside');
  mkdirSync(outside);
  symlinkSync(outside, item.backupRoot);
  try {
    const result = spawnSync('bash', [
      script, 'backup', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--backup-dir', item.backupRoot, '--recipient', 'fixture', '--apply',
    ], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /backup directory|symbolic link/i);
    assert.deepEqual(readdirSync(outside), []);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('GPG backup and restore round-trip only the workspace', { timeout: 30000 }, () => {
  const item = fixture();
  const gnupg = join(item.root, 'gnupg');
  const restore = join(item.root, 'restore');
  mkdirSync(gnupg, { mode: 0o700 });
  const generated = spawnSync('gpg', [
    '--homedir', gnupg, '--batch', '--passphrase', '', '--quick-generate-key',
    'webClx QA Backup <qa-backup@example.invalid>', 'rsa2048', 'encrypt', '1d',
  ], { encoding: 'utf8' });
  assert.equal(generated.status, 0, generated.stderr);
  const fingerprintResult = spawnSync('gpg', [
    '--homedir', gnupg, '--batch', '--with-colons', '--list-keys',
  ], { encoding: 'utf8' });
  const fingerprint = fingerprintResult.stdout.split('\n').find((line) => line.startsWith('fpr:'))?.split(':')[9];
  assert.ok(fingerprint);
  try {
    const backup = spawnSync('bash', [
      script, 'backup', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--backup-dir', item.backupRoot, '--recipient', fingerprint, '--gpg-home', gnupg, '--apply',
    ], { encoding: 'utf8' });
    assert.equal(backup.status, 0, backup.stderr);
    const response = JSON.parse(backup.stdout);
    assert.match(response.backup_file, /\.tar\.gz\.gpg$/);
    assert.equal(existsSync(response.backup_file), true);
    assert.equal(existsSync(`${response.backup_file}.sha256`), true);
    assert.equal(readFileSync(response.backup_file).includes(Buffer.from('synthetic workspace')), false);

    const liveRestore = spawnSync('bash', [
      script, 'restore', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--backup-file', response.backup_file, '--restore-dir', join(item.instance, 'restore-attempt'),
      '--gpg-home', gnupg, '--apply',
    ], { encoding: 'utf8' });
    assert.equal(liveRestore.status, 2);
    assert.match(liveRestore.stderr, /live instance/i);
    assert.equal(existsSync(join(item.instance, 'restore-attempt')), false);

    const restoreResult = spawnSync('bash', [
      script, 'restore', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--backup-file', response.backup_file, '--restore-dir', restore, '--gpg-home', gnupg, '--apply',
    ], { encoding: 'utf8' });
    assert.equal(restoreResult.status, 0, restoreResult.stderr);
    assert.equal(readFileSync(join(restore, 'workspace', 'source.txt'), 'utf8'), 'synthetic workspace\n');
    assert.equal(existsSync(join(restore, 'artifacts')), false);

    const nonempty = join(item.root, 'nonempty-restore');
    mkdirSync(nonempty);
    writeFileSync(join(nonempty, 'keep.txt'), 'do not overwrite\n');
    const refused = spawnSync('bash', [
      script, 'restore', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--backup-file', response.backup_file, '--restore-dir', nonempty, '--gpg-home', gnupg, '--apply',
    ], { encoding: 'utf8' });
    assert.equal(refused.status, 1);
    assert.match(refused.stderr, /must be empty/i);
    assert.equal(readFileSync(join(nonempty, 'keep.txt'), 'utf8'), 'do not overwrite\n');

    appendFileSync(response.backup_file, 'tampered');
    const checksumFailure = spawnSync('bash', [
      script, 'restore', '--customer-id', 'qa-demo-01', '--root-dir', join(item.root, 'instances'),
      '--backup-file', response.backup_file, '--restore-dir', join(item.root, 'tampered-restore'),
      '--gpg-home', gnupg, '--apply',
    ], { encoding: 'utf8' });
    assert.equal(checksumFailure.status, 1);
    assert.match(checksumFailure.stderr, /checksum mismatch/i);
    assert.equal(existsSync(join(item.root, 'tampered-restore')), false);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});
