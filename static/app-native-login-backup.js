(() => {
  "use strict";
  let activeKind = "oauth";
  let generation = 0;
  const labels = { oauth: "Codex OAuth", api: "Codex API", claude: "Claude（OAuth / API）", codebuddy: "CodeBuddy", devin: "Devin CLI", agy: "AGY", opencode: "OpenCode", codearts: "CodeArts", qoder: "Qoder" };
  const status = () => document.getElementById("native-login-status");
  function show(text, tone = "muted") {
    const node = status();
    if (node) { node.textContent = text; node.dataset.tone = tone; }
  }
  async function inspect() {
    const kind = activeKind;
    const current = ++generation;
    show(`正在读取 ${labels[kind]} 本机登录…`);
    try {
      const result = await requestJson(`/api/auth/native-login/${encodeURIComponent(kind)}`);
      if (current !== generation) return;
      window.updateNativeLoginAccounts?.(kind, result);
      const accountLabel = result?.account?.email || result?.account?.name || "";
      const detail = result.available
        ? `已检测到本机登录凭据${accountLabel ? `（${accountLabel}）` : ""}，可下载备份`
        : "未检测到可备份凭据，请先在原生客户端登录";
      show(`${labels[kind]}：${detail}。凭据存在不代表远端登录仍有效。`, result.available ? "ok" : "muted");
    } catch (error) {
      if (current === generation) {
        window.updateNativeLoginAccounts?.(kind, null);
        show(`读取 ${labels[kind]} 失败：${error.message}`, "warn");
      }
    }
  }
  async function download() {
    const kind = activeKind;
    const current = ++generation;
    const button = document.getElementById("native-login-download");
    button.disabled = true;
    show(`正在备份 ${labels[kind]} 本机登录…`);
    try {
      // Re-read on the server: a prior inspection is never the backup source.
      const result = await requestJson(`/api/auth/native-login/${encodeURIComponent(kind)}`, { method: "POST" });
      const blob = new Blob([JSON.stringify(result, null, 2) + "\n"], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `webclx-${kind}-login-${result.created_at}.json`;
      document.body.appendChild(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      if (current === generation) show(`${labels[kind]} 登录备份已生成并发起下载。文件含完整凭据，请妥善保管；当前登录未改动。`, "ok");
    } catch (error) {
      if (current === generation) show(`备份 ${labels[kind]} 失败：${error.message}`, "warn");
    } finally {
      button.disabled = false;
    }
  }
  window.nativeLoginBackup = {
    download,
    select(kind) {
      if (!Object.hasOwn(labels, kind)) return;
      activeKind = kind;
      window.codeartsOptions?.select(kind);
      const login = document.getElementById("native-login-codearts-auth");
      if (login) {
        login.hidden = kind !== "codearts";
        if (kind === "codearts") login.href = "https://codearts.huaweicloud.com/portal/settings/cli-auth";
        else login.removeAttribute("href");
      }
      void inspect();
    },
  };
  window.addEventListener("DOMContentLoaded", () => {
    document.getElementById("native-login-read")?.addEventListener("click", inspect);
    document.getElementById("native-login-download")?.addEventListener("click", download);
  });
})();
