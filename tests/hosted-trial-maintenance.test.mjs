import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const script = new URL('../scripts/hosted-trial-maintenance.sh', import.meta.url).pathname;
const service = new URL('../ops/hosted-trial/webclx-trial-maintenance.service', import.meta.url).pathname;
const timer = new URL('../ops/hosted-trial/webclx-trial-maintenance.timer', import.meta.url).pathname;
const deploy = new URL('../scripts/deploy-hosted-trial-maintenance.sh', import.meta.url).pathname;

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'webclx-maintenance-'));
  const configs = join(root, 'configs');
  const instances = join(root, 'instances');
  const backups = join(root, 'backups');
  const calls = join(root, 'calls.log');
  const guard = join(root, 'guard.sh');
  mkdirSync(configs);
  mkdirSync(join(instances, 'qa-demo-01'), { recursive: true });
  writeFileSync(join(configs, 'qa-demo-01.conf'), [
    'customer_id=qa-demo-01',
    'max_mib=512',
    'backup_recipient=0123456789ABCDEF0123456789ABCDEF01234567',
    `gpg_home=${join(root, 'gnupg')}`,
    'retention_count=2',
  ].join('\n'));
  chmodSync(join(configs, 'qa-demo-01.conf'), 0o600);
  writeFileSync(guard, `#!/usr/bin/env bash\nprintf '%s\\n' "$*" >> ${JSON.stringify(calls)}\n`);
  chmodSync(guard, 0o700);
  return { root, configs, instances, backups, calls, guard };
}

test('maintenance rejects symbolic-link and overly permissive customer configs', () => {
  const item = fixture();
  try {
    const target = join(item.configs, 'qa-demo-01.conf');
    chmodSync(target, 0o644);
    let result = spawnSync('bash', [script, 'capacity', '--config-dir', item.configs,
      '--root-dir', item.instances, '--guard-script', item.guard, '--dry-run'], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /permission|0600|private/i);

    chmodSync(target, 0o600);
    const link = join(item.configs, 'qa-linked.conf');
    symlinkSync(target, link);
    result = spawnSync('bash', [script, 'capacity', '--config-dir', item.configs,
      '--root-dir', item.instances, '--guard-script', item.guard, '--dry-run'], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /symbolic link/i);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('capacity passes the configured customer limit to the data guard', () => {
  const item = fixture();
  try {
    const result = spawnSync('bash', [script, 'capacity', '--config-dir', item.configs,
      '--root-dir', item.instances, '--guard-script', item.guard, '--dry-run'], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const call = readFileSync(item.calls, 'utf8');
    assert.match(call, /^enforce --customer-id qa-demo-01 /);
    assert.match(call, /--max-mib 512 --dry-run$/m);
    assert.doesNotMatch(result.stdout + result.stderr, /0123456789ABCDEF/);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('backup prunes only complete old backup pairs within the customer directory', () => {
  const item = fixture();
  const customerBackups = join(item.backups, 'qa-demo-01');
  mkdirSync(customerBackups, { recursive: true });
  const names = [
    'qa-demo-01-workspace-20260811T000000Z.tar.gz.gpg',
    'qa-demo-01-workspace-20260812T000000Z.tar.gz.gpg',
    'qa-demo-01-workspace-20260813T000000Z.tar.gz.gpg',
  ];
  for (const name of names) {
    writeFileSync(join(customerBackups, name), name);
    writeFileSync(join(customerBackups, `${name}.sha256`), `${'0'.repeat(64)}  ${name}\n`);
  }
  writeFileSync(join(customerBackups, 'keep.txt'), 'unrelated\n');
  try {
    const result = spawnSync('bash', [script, 'backup', '--config-dir', item.configs,
      '--root-dir', item.instances, '--backup-root', item.backups,
      '--guard-script', item.guard, '--apply'], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(existsSync(join(customerBackups, names[0])), false);
    assert.equal(existsSync(join(customerBackups, `${names[0]}.sha256`)), false);
    assert.deepEqual(readdirSync(customerBackups).sort(), [
      'keep.txt', names[1], `${names[1]}.sha256`, names[2], `${names[2]}.sha256`,
    ].sort());
    const call = readFileSync(item.calls, 'utf8');
    assert.match(call, /^backup --customer-id qa-demo-01 /);
    assert.match(call, /--recipient 0123456789ABCDEF0123456789ABCDEF01234567 /);
    assert.match(call, /--apply$/m);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('run enforces hourly but performs backup only once per UTC date', () => {
  const item = fixture();
  const state = join(item.root, 'state');
  mkdirSync(join(item.backups, 'qa-demo-01'), { recursive: true });
  try {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      const result = spawnSync('bash', [script, 'run', '--config-dir', item.configs,
        '--root-dir', item.instances, '--backup-root', item.backups, '--state-dir', state,
        '--guard-script', item.guard, '--backup-hour', '0', '--apply'], { encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
    }
    const calls = readFileSync(item.calls, 'utf8').trim().split('\n');
    assert.equal(calls.filter((line) => line.startsWith('enforce ')).length, 2);
    assert.equal(calls.filter((line) => line.startsWith('backup ')).length, 1);
    assert.match(readFileSync(join(state, 'last-backup-date'), 'utf8'), /^\d{4}-\d{2}-\d{2}\n$/);
  } finally {
    rmSync(item.root, { recursive: true, force: true });
  }
});

test('systemd maintenance schedule is persistent, bounded, and hardened', () => {
  const serviceBody = readFileSync(service, 'utf8');
  const timerBody = readFileSync(timer, 'utf8');
  assert.match(serviceBody, /ExecStart=.*hosted-trial-maintenance\.sh run/);
  assert.match(serviceBody, /NoNewPrivileges=true/);
  assert.match(serviceBody, /ProtectSystem=strict/);
  assert.match(timerBody, /OnCalendar=hourly/);
  assert.match(timerBody, /Persistent=true/);
  assert.match(timerBody, /RandomizedDelaySec=/);
});

test('deployment installs the ExecStart script before verifying systemd units', () => {
  const body = readFileSync(deploy, 'utf8');
  const install = body.indexOf('install -m 0755 "$REMOTE_DIR/hosted-trial-maintenance.sh"');
  const verify = body.indexOf('systemd-analyze verify');
  const reload = body.indexOf('systemctl daemon-reload');
  assert.ok(install >= 0 && verify > install && reload > verify);
  assert.match(body, /systemctl disable --now webclx-trial-maintenance\.timer/);
  assert.doesNotMatch(body, /43\.153\.46\.27|fpsq\.xyz/);
  assert.match(body, /TARGET=.*\$\{1:-\}/);
});
