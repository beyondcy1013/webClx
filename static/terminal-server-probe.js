// ─────────────────────────────────────────────
// webClx terminal server probe — after repeated
// WebSocket failures or network changes, re-evaluate
// which candidate server is fastest instead of blindly
// retrying the current host.
//
// Probing uses mode:"no-cors" fetch to bypass CORS.
// The response body is opaque (unreadable), but a resolved
// promise proves the TCP connection succeeded and the
// server responded.  A rejected promise means the host is
// truly unreachable (DNS failure / connection refused / timeout).
// On Android, a native bridge method is preferred when present
// because it has no CORS restriction and reports accurate latency.
// ─────────────────────────────────────────────

const TERMINAL_SERVER_PROBE = (() => {
  "use strict";

  const DEFAULT_PORT = "11111";
  const PROBE_TIMEOUT_MS = 3500;
  const MIN_PROBE_INTERVAL_MS = 8000;
  const PROBE_PATH = "/favicon.svg";

  let cachedCandidates = null;

  function candidateHosts() {
    if (cachedCandidates) return cachedCandidates;
    const select = document.getElementById("terminal-server-switch-select");
    const fromSelect = select
      ? [...select.querySelectorAll("option")].map((opt) => opt.value).filter(Boolean)
      : [];
    cachedCandidates = [...new Set(["192.168.3.2", ...fromSelect])];
    return cachedCandidates;
  }

  function originFor(host) {
    if (/^https?:\/\//i.test(host)) return new URL(host).origin;
    const withPort = host.includes(":") ? host : `${host}:${DEFAULT_PORT}`;
    return new URL(`http://${withPort}`).origin;
  }

  function isCurrentHost(host) {
    return window.location.origin === originFor(host);
  }

  /**
   * Probe via Android native bridge (no CORS, accurate latency).
   * The bridge returns a JSON string like {"ok":true,"latency":12}.
   * Returns null if bridge is unavailable.
   */
  function probeHostViaNative(host) {
    const bridge = window.WebClxAndroid;
    if (!bridge || typeof bridge.probeHost !== "function") return null;
    try {
      const origin = originFor(host);
      const raw = bridge.probeHost(origin + PROBE_PATH, PROBE_TIMEOUT_MS);
      if (!raw) return { host, ok: false, latency: Infinity };
      const parsed = typeof raw === "string" ? JSON.parse(raw) : raw;
      if (parsed && typeof parsed === "object") {
        return {
          host,
          ok: Boolean(parsed.ok),
          latency: typeof parsed.latency === "number" ? parsed.latency : 0,
        };
      }
      return { host, ok: Boolean(parsed), latency: 0 };
    } catch (_) {
      return { host, ok: false, latency: Infinity };
    }
  }

  /**
   * Probe via no-cors fetch — works cross-origin in all browsers/WebViews.
   * Resolved ⇒ server reachable; rejected ⇒ unreachable.
   */
  async function probeHostViaFetch(host) {
    const url = `${originFor(host)}${PROBE_PATH}?_probe=${Date.now()}`;
    const start = performance.now();
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
      await fetch(url, {
        mode: "no-cors",
        credentials: "omit",
        cache: "no-store",
        redirect: "follow",
        signal: controller.signal,
      });
      clearTimeout(timer);
      return { host, ok: true, latency: performance.now() - start };
    } catch (_) {
      return { host, ok: false, latency: Infinity };
    }
  }

  async function probeHost(host) {
    // Prefer native bridge on Android — synchronous, no CORS, accurate latency.
    const native = probeHostViaNative(host);
    if (native) return native;
    // Fallback: no-cors fetch — universal, cross-origin.
    return probeHostViaFetch(host);
  }

  let probeGeneration = 0;
  let lastProbeAt = 0;

  async function findBestServer(reason = "") {
    const candidates = candidateHosts();
    if (candidates.length === 0) return null;

    const now = Date.now();
    if (now - lastProbeAt < MIN_PROBE_INTERVAL_MS) {
      console.debug("server-probe: debounced, skipping");
      return null;
    }
    lastProbeAt = now;

    const gen = ++probeGeneration;
    const currentOrigin = window.location.origin;
    console.debug(`server-probe: probing current server first (${reason})`);

    const currentResult = await probeHost(currentOrigin);
    if (gen !== probeGeneration) return null;
    if (currentResult.ok) {
      console.debug(`server-probe: current server reachable (${Math.round(currentResult.latency)}ms), staying`);
      return null;
    }

    const alternatives = candidates.filter((host) => originFor(host) !== currentOrigin);
    console.debug(`server-probe: current server unavailable, probing ${alternatives.length} alternatives`);
    const results = await Promise.all(alternatives.map(probeHost));
    if (gen !== probeGeneration) return null;

    const reachable = results.filter((r) => r.ok);
    if (reachable.length === 0) {
      console.debug("server-probe: no candidate reachable");
      return null;
    }

    reachable.sort((a, b) => a.latency - b.latency);
    const best = reachable[0];
    console.debug(`server-probe: fastest reachable = ${best.host} (${Math.round(best.latency)}ms)`);
    return best.host;
  }

  function switchToServer(host) {
    if (typeof navigateToTerminalServer === "function") {
      navigateToTerminalServer(host);
    }
  }

  async function reevaluate(reason = "") {
    const best = await findBestServer(reason);
    if (best && !isCurrentHost(best)) {
      switchToServer(best);
      return true;
    }
    return false;
  }

  return { candidateHosts, findBestServer, switchToServer, reevaluate };
})();
