import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const deployScript = readFileSync(
  new URL(
    "../.codex/skills/webclx-nas-deploy/scripts/deploy-built-artifact.sh",
    import.meta.url,
  ),
  "utf8",
);

test("NAS deploy uses the current target and installs a non-recursive launcher", () => {
  assert.match(deployScript, /^HOST="192\.168\.3\.250"$/m);
  assert.doesNotMatch(deployScript, /192\.168\.3\.107/);
  assert.match(
    deployScript,
    /^exec \/data_n004\/webclx\/runtime\/webClx "\$@"$/m,
  );
  assert.doesNotMatch(
    deployScript,
    /^exec \/data_n004\/webclx\/start-webclx\.sh/m,
  );

  const launcherInstall = deployScript.indexOf(
    'sudo -n tee "$LAUNCHER.next"',
  );
  const serviceStop = deployScript.indexOf(
    'sudo -n systemctl stop "$SERVICE"',
  );
  assert.ok(launcherInstall >= 0, "launcher installation is missing");
  assert.ok(serviceStop > launcherInstall, "launcher must be repaired before stop");
});
