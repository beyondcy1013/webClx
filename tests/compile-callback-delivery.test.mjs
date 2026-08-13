import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

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
  /--arg delivery_id[\s\S]*bracketed_paste:true[\s\S]*verify_submission:true[\s\S]*delivery_id:\$delivery_id/,
  "compile callbacks must request bracketed agent-prompt framing and rollout verification",
);
assert.match(
  notifyFunction,
  /jq -e '\.ok == true and \.submitted == true'/,
  "compile callbacks must accept only responses with submitted=true",
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
  /notify_terminal "\$notify_target" "\$message" "\$request_id"/,
  "the unique compile request id must be used as the delivery acknowledgement key",
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

console.log("compile callback delivery contract tests passed");
