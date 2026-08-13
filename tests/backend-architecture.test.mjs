import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '..');

const expectedRoutes = new Set([
  '/',
  '/agent',
  '/api/agent/api-presets',
  '/api/agent/config',
  '/api/agent/exec-with-preset',
  '/api/agent/models',
  '/api/agent/sessions',
  '/api/agent/sessions/{session_id}',
  '/api/agent/sessions/{session_id}/approvals',
  '/api/agent/sessions/{session_id}/approvals/allow-all',
  '/api/agent/sessions/{session_id}/approvals/{approval_id}/allow',
  '/api/agent/sessions/{session_id}/approvals/{approval_id}/deny',
  '/api/agent/sessions/{session_id}/chat',
  '/api/agent/sessions/{session_id}/chat/stop',
  '/api/agent/sessions/{session_id}/checkpoints',
  '/api/agent/sessions/{session_id}/checkpoints/{checkpoint_id}/restore',
  '/api/agent/sessions/{session_id}/clear',
  '/api/agent/sessions/{session_id}/commands',
  '/api/agent/sessions/{session_id}/commands/{command_id}',
  '/api/agent/sessions/{session_id}/commands/{command_id}/stdin',
  '/api/agent/sessions/{session_id}/commands/{command_id}/terminate',
  '/api/agent/sessions/{session_id}/compact',
  '/api/agent/sessions/{session_id}/context',
  '/api/agent/sessions/{session_id}/run',
  '/api/agent/sessions/{session_id}/summary',
  '/api/agent/skill-dirs',
  '/api/agent/skills',
  '/api/agent/skills/toggle',
  '/api/agent/terminal-profiles',
  '/api/agent/terminal-profiles/{profile_id}',
  '/api/agent/tools',
  '/api/artifacts',
  '/api/artifacts/download/{artifact_id}/{file_name}',
  '/api/artifacts/publish',
  '/api/artifacts/update/android/{project}',
  '/api/auth/api-presets',
  '/api/auth/api-presets/import',
  '/api/auth/api-presets/import-file',
  '/api/auth/api-presets/reorder',
  '/api/auth/api-presets/test-all',
  '/api/auth/api-presets/{preset_id}',
  '/api/auth/api-presets/{preset_id}/apply',
  '/api/auth/api-presets/{preset_id}/test',
  '/api/auth/api-presets/{preset_id}/verify',
  '/api/auth/claude-presets',
  '/api/auth/claude-presets/reorder',
  '/api/auth/claude-presets/test-all',
  '/api/auth/claude-presets/{preset_id}',
  '/api/auth/claude-presets/{preset_id}/apply',
  '/api/auth/claude-presets/{preset_id}/apply-opencode',
  '/api/auth/claude-presets/{preset_id}/test',
  '/api/auth/current',
  '/api/auth/login',
  '/api/auth/logout',
  '/api/auth/oauth/codex/sessions/{session_id}',
  '/api/auth/oauth/codex/start',
  '/api/auth/preset-test-schedules',
  '/api/auth/preset-test-schedules/{schedule_id}',
  '/api/auth/preset-test-schedules/{schedule_id}/run',
  '/api/auth/presets',
  '/api/auth/presets/refresh-all-quotas',
  '/api/auth/presets/reorder',
  '/api/auth/presets/test-all',
  '/api/auth/presets/{preset_id}',
  '/api/auth/presets/{preset_id}/apply',
  '/api/auth/presets/{preset_id}/refresh-quota',
  '/api/auth/presets/{preset_id}/test',
  '/api/auth/preset-run-leases',
  '/api/auth/preset-run-leases/{lease_id}',
  '/api/auth/preset-run-leases/{lease_id}/heartbeat',
  '/api/auth/session',
  '/api/auth/upstream-proxy-settings',
  '/api/build/compile',
  '/api/build/compile/complete',
  '/api/build/compile/notify',
  '/api/build/compile/status',
  '/api/build/deploy',
  '/api/codex/tasks',
  '/api/codex/tasks/{task_id}',
  '/api/codex-proxy/anthropic/v1/responses',
  '/api/codex-proxy/deepseek/v1/responses',
  '/api/codex-proxy/minimax/v1/responses',
  '/api/codex-proxy/zhipu/v1/responses',
  '/api/codex_apis',
  '/api/entries',
  '/api/file',
  '/api/file/rename',
  '/api/frp/roles',
  '/api/frp/roles/{id}',
  '/api/frp/roles/{id}/download',
  '/api/frp/roles/{id}/restart',
  '/api/frp/roles/{id}/start',
  '/api/frp/roles/{id}/stop',
  '/api/frp/roles/{id}/unmanage',
  '/api/frp/system',
  '/api/frp/system/adopt',
  '/api/frp/test-port',
  '/api/frpc',
  '/api/frpc/download',
  '/api/frpc/restart',
  '/api/frpc/start',
  '/api/frpc/stop',
  '/api/frps',
  '/api/frps/download',
  '/api/frps/restart',
  '/api/frps/start',
  '/api/frps/stop',
  '/api/proxy/active',
  '/api/proxy/presets',
  '/api/proxy/presets/reorder',
  '/api/proxy/presets/{preset_id}',
  '/api/proxy/test',
  '/api/quota/config',
  '/api/quota/query',
  '/api/service/deploy',
  '/api/settings',
  '/api/settings/codex-common-config',
  '/api/settings/config-file',
  '/api/settings/merge-all',
  '/api/settings/merge-field',
  '/api/settings/merge-tab',
  '/api/settings/preset-config',
  '/api/settings/preset-config/clipboard/{section}/export',
  '/api/settings/preset-config/clipboard/{section}/import',
  '/api/settings/preset-config/import-remote',
  '/api/settings/preset-config/remote-preview',
  '/api/system/info',
  '/api/system/logs',
  '/api/system/proxy',
  '/api/system/restart',
  '/api/system/save-and-poweroff',
  '/api/system/save-and-restart',
  '/api/terminal/auto-continue-tasks',
  '/api/terminal/auto-continue-tasks/{marker}',
  '/api/terminal/auto-typed-input',
  '/api/terminal/codex-conversations',
  '/api/terminal/codex-conversations/model',
  '/api/terminal/codex-conversations/{session_id}',
  '/api/terminal/codex-full-access',
  '/api/terminal/completion-bell.wav',
  '/api/terminal/quick-command',
  '/api/terminal/resume-archives',
  '/api/terminal/resume-archives/{archive_id}',
  '/api/terminal/resume-archives/{archive_id}/touch',
  '/api/terminal/scheduled-inputs',
  '/api/terminal/scheduled-inputs/{task_id}',
  '/api/terminal/sessions',
  '/api/terminal/sessions/message',
  '/api/terminal/sessions/search',
  '/api/terminal/sessions/{session_id}',
  '/api/terminal/sessions/{session_id}/agent-session',
  '/api/terminal/sessions/{session_id}/agent-session/complete',
  '/api/terminal/sessions/{session_id}/agents-doc',
  '/api/terminal/sessions/{session_id}/agents-docs',
  '/api/terminal/sessions/{session_id}/auto-continue',
  '/api/terminal/sessions/{session_id}/continue',
  '/api/terminal/sessions/{session_id}/current-directory',
  '/api/terminal/sessions/{session_id}/extract-preset',
  '/api/terminal/sessions/{session_id}/idle',
  '/api/terminal/sessions/{session_id}/input',
  '/api/terminal/sessions/{session_id}/input-history',
  '/api/terminal/sessions/{session_id}/interrupt-and-resume',
  '/api/terminal/sessions/{session_id}/paste-assets',
  '/api/terminal/sessions/{session_id}/restore',
  '/api/terminal/ws',
  '/api/update/check',
  '/api/update/download',
  '/api/workspace-icon',
  '/api/upstream/anthropic',
  '/api/upstream/anthropic/{*proxy_path}',
  '/api/upstream/openai/v1',
  '/api/upstream/openai/v1/{*proxy_path}',
  '/assets/{*asset_path}',
  '/downloads',
  '/login',
  '/terminal',
]);

function rustFilesUnder(relativeDir) {
  const root = path.join(repoRoot, relativeDir);
  if (!fs.existsSync(root)) return [];

  return fs.readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const relativePath = path.join(relativeDir, entry.name);
    return entry.isDirectory()
      ? rustFilesUnder(relativePath)
      : entry.isFile() && entry.name.endsWith('.rs')
        ? [relativePath]
        : [];
  });
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('route registration is composed by domain without changing the route set', () => {
  const expectedDomainFiles = [
    'src/routes/agent.rs',
    'src/routes/artifacts.rs',
    'src/routes/auth.rs',
    'src/routes/codex_task.rs',
    'src/routes/frp.rs',
    'src/routes/gateway.rs',
    'src/routes/operations.rs',
    'src/routes/pages.rs',
    'src/routes/system.rs',
    'src/routes/terminal.rs',
    'src/routes/workspace.rs',
  ];
  const routeFiles = rustFilesUnder('src/routes');

  for (const relativePath of expectedDomainFiles) {
    assert.ok(routeFiles.includes(relativePath), `missing domain route module: ${relativePath}`);
  }

  const routeSource = routeFiles.map(read).join('\n');
  const registrations = [...routeSource.matchAll(/\.route\(\s*"([^"]+)"/g)].map(
    (match) => match[1],
  );

  assert.equal(registrations.length, 180);
  assert.deepEqual(new Set(registrations), expectedRoutes);
  assert.doesNotMatch(read('src/main.rs'), /\.route\(/);
});

test('upstream protocol transforms live behind a private child module', () => {
  const rootSource = read('src/upstream_proxy.rs');
  const transformPath = 'src/upstream_proxy/transform.rs';
  const testsPath = 'src/upstream_proxy/tests.rs';

  assert.match(rootSource, /^mod transform;$/m);
  assert.match(rootSource, /^mod tests;$/m);
  assert.ok(fs.existsSync(path.join(repoRoot, transformPath)), `missing ${transformPath}`);
  assert.ok(fs.existsSync(path.join(repoRoot, testsPath)), `missing ${testsPath}`);
  assert.doesNotMatch(rootSource, /^fn anthropic_messages_request_to_openai_chat\(/m);
  assert.doesNotMatch(rootSource, /^fn openai_responses_payload_to_anthropic_messages_response\(/m);
  assert.match(read(transformPath), /pub\(super\) fn anthropic_messages_request_to_openai_chat\(/);
  assert.ok(rootSource.split('\n').length < 1200, 'upstream proxy root still mixes test code');
});

test('audited production modules do not use direct unwrap or expect calls', () => {
  const auditedFiles = [
    'src/terminal.rs',
    'src/upstream_proxy.rs',
    'src/frpc.rs',
    'src/auth.rs',
    'src/agent.rs',
    'src/compile_service.rs',
    'src/proxy.rs',
    'src/llm.rs',
    'src/main.rs',
    'src/startup_tools.rs',
  ];
  const panicCalls = [];

  for (const relativePath of auditedFiles) {
    const source = read(relativePath);
    const testModuleStart = source.search(/\n#\[cfg\(test\)\]\nmod [A-Za-z0-9_]+/);
    const productionSource = testModuleStart >= 0 ? source.slice(0, testModuleStart) : source;
    for (const match of productionSource.matchAll(/\.(unwrap|expect)\(/g)) {
      const line = productionSource.slice(0, match.index).split('\n').length;
      panicCalls.push(`${relativePath}:${line} .${match[1]}()`);
    }
  }

  assert.deepEqual(panicCalls, []);
});

test('Codex conversation deletion keeps client statuses outside the terminal task boundary', () => {
  const terminalSource = read('src/terminal.rs');
  const start = terminalSource.indexOf('pub async fn delete_codex_conversation(');
  const end = terminalSource.indexOf('pub async fn save_resume_archive(', start);
  assert.ok(start >= 0 && end > start, 'missing Codex conversation delete handler');
  const handler = terminalSource.slice(start, end);

  assert.match(
    handler,
    /let normalized_session_id = validated_codex_session_id\(&session_id\)[\s\S]*?let terminal_manager[\s\S]*?run_terminal_task/,
    'invalid session ids should be rejected before starting a terminal background task',
  );
  assert.match(
    handler,
    /let resume_session_is_active = run_terminal_task\([\s\S]*?\.await\?;[\s\S]*?if resume_session_is_active \{[\s\S]*?AppError::bad_request/,
    'active-session conflicts should retain their 400 status outside run_terminal_task',
  );
  assert.match(
    handler,
    /let deleted = run_terminal_task\([\s\S]*?\.await\?;[\s\S]*?if deleted\.total_deleted\(\) == 0 \{[\s\S]*?AppError::not_found/,
    'missing conversations should retain their 404 status outside run_terminal_task',
  );
});
