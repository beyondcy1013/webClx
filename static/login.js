(() => {
  const form = document.getElementById("login-form");
  const userInput = document.getElementById("login-username");
  const passInput = document.getElementById("login-password");
  const errorEl = document.getElementById("login-error");
  const submitBtn = document.getElementById("login-submit");
  const originalLabel = submitBtn ? submitBtn.textContent : "登录";

  function t(message) {
    return window.webclxI18n?.translate?.(message) || message;
  }

  // 从 URL 参数读取登录后跳转目标，默认回首页。
  function redirectTarget() {
    const params = new URLSearchParams(window.location.search);
    const next = params.get("next");
    if (next && next.startsWith("/") && !next.startsWith("//")) {
      return next;
    }
    return "/";
  }

  function showError(message) {
    if (!errorEl) return;
    errorEl.textContent = message;
    errorEl.hidden = false;
  }

  function setBusy(busy) {
    if (!submitBtn) return;
    submitBtn.disabled = busy;
    submitBtn.textContent = busy ? t("登录中…") : t(originalLabel);
  }

  async function redirectAuthenticatedSession() {
    try {
      const response = await fetch("/api/auth/session", { cache: "no-store" });
      if (!response.ok) return;
      const session = await response.json();
      if (session?.authenticated) {
        window.location.assign(redirectTarget());
      }
    } catch {}
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    errorEl.hidden = true;
    setBusy(true);

    const username = userInput.value.trim();
    const password = passInput.value;

    if (!username || !password) {
      showError(t("请输入账号和密码"));
      setBusy(false);
      return;
    }

    try {
      const response = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ username, password }),
      });

      if (!response.ok) {
        const message = await response.text();
        showError(t(message || "登录失败，请检查账号和密码"));
        setBusy(false);
        return;
      }

      // 登录成功，cookie 已由服务端设置，跳转目标页。
      window.location.assign(redirectTarget());
    } catch (error) {
      showError(t("网络错误，请稍后重试"));
      setBusy(false);
    }
  });

  // 自动聚焦用户名输入框。
  userInput.focus();
  void redirectAuthenticatedSession();
})();
