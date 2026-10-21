import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const worker = readFileSync(
  new URL(
    "../docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh",
    import.meta.url,
  ),
  "utf8",
);
const operationsRoutes = readFileSync(
  new URL("../src/routes/operations.rs", import.meta.url),
  "utf8",
);
const compileService = readFileSync(
  new URL("../src/compile_service.rs", import.meta.url),
  "utf8",
);

assert.match(
  operationsRoutes,
  /"\/api\/build\/compile\/complete"[\s\S]*compile_service::complete_compile_request/,
  "the operations router must expose the build lifecycle completion endpoint",
);
assert.match(
  compileService,
  /pub async fn complete_compile_request[\s\S]*complete_pending_build_request/,
  "the lifecycle completion endpoint must clear the pending request registry",
);

const notifyFunction = worker.slice(
  worker.indexOf("notify_terminal()"),
  worker.indexOf("notify_terminal_toast()"),
);
assert.match(
  notifyFunction,
  /--arg delivery_id[\s\S]*--arg completed_build_request_id[\s\S]*bracketed_paste:true[\s\S]*verify_submission:true[\s\S]*delivery_id:\$delivery_id[\s\S]*completed_build_request_id:\$completed_build_request_id/,
  "compile callbacks must request bracketed agent-prompt framing and rollout verification",
);
assert.match(
  notifyFunction,
  /jq -e '\.ok == true and \.submitted == true'/,
  "compile callbacks must accept only responses with submitted=true",
);
assert.match(
  notifyFunction,
  /while :[\s\S]*refresh_local_auth_args[\s\S]*curl -fsS/,
  "compile callback retries must refresh the local auth token for every attempt",
);
assert.match(
  worker,
  /TERMINAL_CALLBACK_HTTP_TIMEOUT="\$\{WEBCLX_TERMINAL_CALLBACK_HTTP_TIMEOUT:-45\}"/,
  "one callback HTTP request must outlive the server-side rollout verification budget",
);
assert.match(
  notifyFunction,
  /--max-time "\$TERMINAL_CALLBACK_HTTP_TIMEOUT"/,
  "terminal callbacks must not overlap because the client timed out before verification finished",
);
assert.doesNotMatch(
  notifyFunction,
  /--retry "\$CALLBACK_RETRY_COUNT"/,
  "curl must not reuse one authentication header across a webClx restart",
);

const completionFunction = worker.slice(
  worker.indexOf("notify_build_complete()"),
  worker.indexOf("notify_terminal()"),
);
assert.match(
  worker,
  /CALLBACK_RETRY_COUNT="\$\{WEBCLX_CALLBACK_RETRY_COUNT:-300\}"[\s\S]*CALLBACK_RETRY_MAX_TIME="\$\{WEBCLX_CALLBACK_RETRY_MAX_TIME:-300\}"/,
  "compile callbacks must tolerate the normal webClx self-deployment outage window",
);
assert.match(
  completionFunction,
  /--retry "\$CALLBACK_RETRY_COUNT"[\s\S]*--retry-max-time "\$CALLBACK_RETRY_MAX_TIME"/,
  "build lifecycle completion must use the bounded long callback retry policy",
);
assert.match(
  notifyFunction,
  /callback_attempt=\$\(\(callback_attempt \+ 1\)\)[\s\S]*CALLBACK_RETRY_COUNT[\s\S]*callback_deadline/,
  "terminal callbacks must retry transient HTTP errors while restored sessions come back",
);
assert.match(
  completionFunction,
  /\/api\/build\/compile\/complete/,
  "the build worker must report request completion through a dedicated lifecycle endpoint",
);
assert.match(
  completionFunction,
  /request_id[\s\S]*jq -e[\s\S]*\.ok == true/,
  "the build completion endpoint response must be acknowledged",
);
assert.match(
  worker,
  /WEBCLX_LOCAL_TOKEN_FILE[\s\S]*X-WebClx-Local-Token/,
  "the build worker must authenticate loopback callbacks with the local token",
);
assert.match(
  completionFunction,
  /refresh_local_auth_args[\s\S]*"\$\{LOCAL_AUTH_ARGS\[@\]\}"/,
  "the completion callback must refresh and send local authentication after self-deploy",
);

const deliveryBlock = worker.slice(
  worker.indexOf("notify_request_file()"),
  worker.indexOf("collect_command_path_candidates()"),
);
assert.match(
  deliveryBlock,
  /local [^\n]*\bdelivery_id\b[^\n]*\bfinal_message\b[^\n]*\bfallback_message\b/,
  "the request notification must declare its delivery-scoped variables locally",
);
const completionIndex = deliveryBlock.indexOf('notify_build_complete "$request_id"');
const toastIndex = deliveryBlock.indexOf("notify_terminal_toast");
const promptIndex = deliveryBlock.indexOf('notify_terminal "$notify_target"');
assert.ok(
  completionIndex >= 0 &&
    toastIndex >= 0 &&
    promptIndex >= 0 &&
    completionIndex < toastIndex &&
    toastIndex < promptIndex,
  "build completion, toast, and prompt callbacks must be sent in lifecycle order",
);
const missingTargetReturnMatch = /if \[ -z "\$notify_target" \]; then\s+return 0\s+fi/.exec(
  deliveryBlock,
);
const missingTargetReturnIndex = missingTargetReturnMatch?.index ?? -1;
assert.ok(
  missingTargetReturnIndex < 0 || completionIndex < missingTargetReturnIndex,
  "build completion must not depend on resolving a terminal notification target",
);
assert.doesNotMatch(
  deliveryBlock,
  /wait_terminal_ready/,
  "prompt delivery must not wait for a busy terminal to become idle",
);
assert.match(
  deliveryBlock,
  /notify_terminal "\$notify_target" "\$final_message" "\$delivery_id" "\$request_id"/,
  "the final callback must use a request-scoped delivery marker while preserving the build request id",
);
assert.match(
  deliveryBlock,
  /notify_terminal_unverified "\$notify_target" "\$fallback_message" "\$fallback_delivery_id"/,
  "the unverified retry marker must be explicitly distinct from the final delivery marker",
);
assert.doesNotMatch(
  deliveryBlock,
  /"\$\{request_id\}-fallback"/,
  "fallback delivery must not reuse the legacy marker spelling",
);
assert.match(
  deliveryBlock,
  /notification_failed=1[\s\S]*return "\$notification_failed"/,
  "callback failures must propagate out of each request notification task",
);
assert.match(
  worker,
  /if \[ "\$notify_status" -ne 0 \]; then[\s\S]*exit 1/,
  "the worker must fail visibly when completion callbacks remain undeliverable",
);
assert.match(
  worker,
  /trap 'handle_worker_signal TERM 143' TERM/,
  "the worker must finalize pending build lifecycle state when systemd stops it",
);
const interruptedRunFunction = worker.slice(
  worker.indexOf("complete_interrupted_run()"),
  worker.indexOf("handle_worker_signal()"),
);
assert.match(
  interruptedRunFunction,
  /run-interrupted-signal[\s\S]*run-finished-at[\s\S]*rm -f "\$ACTIVE_RUN_DIR\/progress\.json"/,
  "interrupted runs must receive terminal run markers and lose live progress",
);
assert.match(
  interruptedRunFunction,
  /\/api\/build\/compile\/complete/,
  "interrupted runs must clear each request from terminal build activity",
);

// Execute the production request-to-payload path; only external IO and run
// metadata lookup are replaced. No real terminal, HTTP server, or child daemon.
function shellFunction(name) {
  const start = worker.indexOf(`${name}() {`);
  assert.ok(start >= 0, `missing function ${name}`);
  const end = worker.indexOf('\n}\n', start);
  assert.ok(end > start, `missing end of ${name}`);
  return worker.slice(start, end + 3);
}
const functions = [
  'notify_terminal', 'notify_terminal_unverified',
  'compile_completion_delivery_id', 'notify_request_file',
  'json_get', 'notification_subject', 'display_clock_time',
].map(shellFunction).join('\n');

function captureProductionCallbacks(mode) {
  const directory = mkdtempSync(join(tmpdir(), 'webclx-callback-'));
  try {
    writeFileSync(join(directory, 'request.json'), JSON.stringify({
      request_id: 'req-1', source_terminal_id: 'fixture-session',
      project: 'fixture', project_dir: directory, project_path: 'fixture',
      request_kind: 'compile', note: 'ordinary request reference req-1',
    }));
    writeFileSync(join(directory, 'status-fixture'), '0\n');
    const script = String.raw`
set -euo pipefail
run_dir="$1"
mode="$2"
log_file="$run_dir/worker.log"
request_count=1
BASE_URL=http://fixture.invalid
CALLBACK_RETRY_COUNT=2
CALLBACK_RETRY_MAX_TIME=10
TERMINAL_CALLBACK_HTTP_TIMEOUT=1
refresh_local_auth_args() { LOCAL_AUTH_ARGS=(); }
notify_build_complete() { :; }
notify_terminal_toast() { :; }
request_spec() { cat "$1"; }
spec_key() { cat >/dev/null; printf fixture; }
# Retry timing is not under test. Keep the production retry loop intact.
sleep() { :; }
curl() {
  local payload=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = '-d' ]; then payload="$2"; shift 2; else shift; fi
  done
  printf '%s\n' "$payload" >>"$run_dir/payloads.jsonl"
  local count
  count=$(wc -l <"$run_dir/payloads.jsonl")
  if { [ "$mode" = retry ] && [ "$count" -eq 1 ]; } ||
     { [ "$mode" = fallback ] && [ "$count" -le 2 ]; }; then
    printf '%s\n' '{"ok":true,"submitted":false}'
  else
    printf '%s\n' '{"ok":true,"submitted":true}'
  fi
}
` + functions + '\nnotify_request_file "$run_dir/request.json"\n';
    const result = spawnSync('bash', ['-c', script, 'callback-fixture', directory, mode], {
      encoding: 'utf8', timeout: 10000, killSignal: 'SIGKILL',
    });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr || result.stdout);
    return readFileSync(join(directory, 'payloads.jsonl'), 'utf8')
      .trim().split('\n').map(line => JSON.parse(line));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

const [first] = captureProductionCallbacks('success');
assert.equal(first.target, 'fixture-session');
assert.equal(first.completed_build_request_id, 'req-1');
assert.equal(first.verify_submission, true);
assert.equal(first.delivery_id, '[webclx-callback:complete:req-1]');
assert.ok(first.data.includes(first.delivery_id), 'real body must contain the full evidence marker');
assert.ok(first.data.includes('编译完成'), 'exercise production completion body construction');
const history = ['[state] request req-1 queued', 'please check req-1'];
assert.equal(history.some(text => text.includes(first.delivery_id)), false);
history.push(first.data);
assert.equal(history.filter(text => text.includes(first.delivery_id)).length, 1);

const retry = captureProductionCallbacks('retry');
assert.equal(retry.length, 2);
assert.deepEqual(retry[0], retry[1], 'retry preserves exact payload and evidence marker');
const fallback = captureProductionCallbacks('fallback');
assert.equal(fallback.length, 3);
assert.deepEqual(fallback[0], fallback[1]);
assert.equal(fallback[2].verify_submission, false);
assert.equal(fallback[2].completed_build_request_id, undefined);
assert.ok(fallback[2].data.includes(fallback[2].delivery_id));
assert.equal(fallback[2].data.includes(first.delivery_id), false,
  'unverified fallback must not be evidence for verified completion');
assert.equal(first.data.includes(fallback[2].delivery_id), false);
console.log('compile callback contracts and production payload behavior tests passed');
