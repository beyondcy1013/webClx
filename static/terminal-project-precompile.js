// Project-owned build settings; the existing compile queue owns execution and callbacks.
const projectPrecompileSubmissions = new Set();

function parseProjectPrecompileConfig(content) {
  const config = JSON.parse(content);
  if (!config || typeof config !== "object" || Array.isArray(config)) {
    throw new Error(".webclx.json 必须是 JSON 对象。");
  }
  if (config.precompile === undefined) return {};
  const precompile = config.precompile;
  if (!precompile || typeof precompile !== "object" || Array.isArray(precompile)) {
    throw new Error(".webclx.json 的 precompile 必须是对象。");
  }
  const command = precompile.command;
  if (!Array.isArray(command) || command.length === 0
    || command.some((argument) => typeof argument !== "string" || argument.includes("\0"))
    || !command[0].trim()) {
    throw new Error("precompile.command 必须是非空命令参数数组，例如 [\"cargo\", \"build\", \"--release\", \"--locked\"]。");
  }
  const payload = { command: [...command] };
  if (precompile.required_artifacts !== undefined) {
    const artifacts = precompile.required_artifacts;
    if (!Array.isArray(artifacts) || artifacts.some((path) =>
      typeof path !== "string" || !path.trim() || path.includes("\0"))) {
      throw new Error("precompile.required_artifacts 必须是非空路径组成的数组。");
    }
    payload.required_artifacts = [...artifacts];
  }
  return payload;
}

async function loadProjectPrecompileConfig(relativePath) {
  const configPath = relativePath ? `${relativePath}/.webclx.json` : ".webclx.json";
  const response = await fetch(`/api/file?path=${encodeURIComponent(configPath)}`);
  // Only an actual missing file allows inference; access/read/parse errors remain visible.
  if (response.status === 404) return {};
  if (!response.ok) {
    throw new Error(`读取预编译配置失败：${await response.text() || response.status}`);
  }
  const file = await response.json();
  if (file.editable === false) throw new Error(file.message || "预编译配置无法读取。");
  try {
    return parseProjectPrecompileConfig(file.content);
  } catch (error) {
    throw new Error(`${configPath}：${error.message}`);
  }
}

async function triggerProjectPrecompile() {
  // Capture the origin before any await: switching terminals must not retarget a build.
  const relativePath = normalizeRelativePath(state.currentPath || "");
  const configuredRoot = String(state.workspaceDir || "").trim();
  if (!configuredRoot) {
    updateStatus("终端设置尚未加载，无法定位预编译项目，请稍候再试。", "warn");
    return;
  }
  if (!String(state.activeSessionId || "").trim()) {
    updateStatus("当前没有活动终端，无法发起预编译。", "warn");
    return;
  }
  const workspaceRoot = configuredRoot.replace(/\/+$/, "");
  const projectDir = relativePath ? `${workspaceRoot}/${relativePath}` : workspaceRoot || "/";
  const projectName = projectDir.split("/").filter(Boolean).pop() || "workspace";
  if (projectPrecompileSubmissions.has(projectDir)) {
    updateStatus(`${projectName} 的预编译请求正在提交，请勿重复点击。`, "info");
    return;
  }
  projectPrecompileSubmissions.add(projectDir);
  updateStatus(`正在排队预编译 ${projectName}…`, "info");
  try {
    const sourceTerminal = await resolveCurrentDeploySourceTerminal();
    const buildConfig = await loadProjectPrecompileConfig(relativePath);
    const response = await requestJson("/api/build/compile", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        source_terminal_name: sourceTerminal.name,
        source_terminal_id: sourceTerminal.id,
        source_tmux_session: sourceTerminal.tmuxSessionName,
        project: projectName,
        project_dir: projectDir,
        project_path: relativePath,
        // Marks this as a warm-up build: any later real build request for the
        // same project supersedes and stops it instead of racing over artifacts.
        purpose: "precompile",
        note: "项目管理菜单：预编译预热，复用正式构建配置与缓存；仅编译，不安装、不重启。",
        ...buildConfig,
      }),
    });
    if (response?.queued) {
      const command = Array.isArray(response.command) ? response.command.join(" ") : "项目编译命令";
      updateStatus(
        `预编译已入队：${response.project || projectName}（${response.request_id || "请求号未返回"}）。${command}；仅编译，完成后回调终端 ${sourceTerminal.name}。`,
        "ok",
      );
    } else {
      updateStatus("预编译请求状态未确认，请在编译状态页核对后再重试。", "warn");
    }
  } catch (error) {
    updateStatus(error?.message || "预编译请求失败，请检查编译状态后再重试。", "warn");
  } finally {
    projectPrecompileSubmissions.delete(projectDir);
  }
}
