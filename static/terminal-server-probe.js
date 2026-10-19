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
// Probing always uses fetch: the former synchronous Android bridge
// probe blocked the JS thread on network I/O inside the WebView.
// ─────────────────────────────────────────────

const TERMINAL_SERVER_PROBE = (() => {
  "use strict";

  const DEFAULT_PORT = "11111";
  const PROBE_TIMEOUT_MS = 3500;
  const MIN_PROBE_INTERVAL_MS = 8000;
  const PROBE_PATH = "/favicon.svg";
  // Grace boundary: a short unavailability of the current origin — most
  // commonly the webClx service restarting itself — must never trigger a
  // server switch.  Only when the current server has stayed unreachable for
  // this whole window do we consider alternative candidates.  Any successful
  // probe of (or WebSocket reconnect to) the current origin resets the clock.
  const GRACE_PERIOD_MS = 60000;

  let cachedCandidates = null;
  let currentUnreachableSince = 0;

  function notifyServerReachable() {
    currentUnreachableSince = 0;
  }

  function candidateHosts() {
    if (cachedCandidates) return cachedCandidates;
    const select = document.getElementById("terminal-server-switch-select");
    const fromSelect = select
      ? [...select.querySelectorAll("option")].map((opt) => opt.value).filter(Boolean)
      : [];
    cachedCandidates = [...new Set(["HOST", ...fromSelect])];
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
   * Probe via no-cors fetch — works cross-origin in all browsers/WebViews.
   * Resolved ⇒ server reachable; rejected ⇒ unreachable.
   */
  async function probeHostViaFetch(host) {
    const url = `${originFor(host)}${PROBE_PATH}?_probe=${Date.now()}`;
    const start = performance.now();
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
    try {
      await fetch(url, {
        mode: "no-cors",
        credentials: "omit",
        cache: "no-store",
        redirect: "follow",
        signal: controller.signal,
      });
      return { host, ok: true, latency: performance.now() - start };
    } catch (_) {
      return { host, ok: false, latency: Infinity };
    } finally {
      clearTimeout(timer);
    }
  }

  async function probeHost(host) {
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
      currentUnreachableSince = 0;
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

  let reevaluateInFlight = null;

  function reevaluate(reason = "") {
    // Serialize re-evaluations: a second caller (network callback, resume
    // dispatch, reconnect threshold) coalesces onto the in-flight probe
    // instead of racing duplicate fetches against the same candidates.
    if (reevaluateInFlight) {
      return reevaluateInFlight;
    }
    reevaluateInFlight = runReevaluate(reason).finally(() => {
      reevaluateInFlight = null;
    });
    return reevaluateInFlight;
  }

  async function runReevaluate(reason = "") {
    // Restart-aware boundary: probe the current origin first.  A reachable
    // current server always wins (reset the outage clock and stay).  An
    // unreachable one starts/continues the grace clock; switching to another
    // candidate is only allowed once the outage outlasts GRACE_PERIOD_MS.
    const currentResult = await probeHost(window.location.origin);
    if (currentResult.ok) {
      currentUnreachableSince = 0;
      return false;
    }
    const now = Date.now();
    if (!currentUnreachableSince) {
      currentUnreachableSince = now;
    }
    const unreachableForMs = now - currentUnreachableSince;
    if (unreachableForMs < GRACE_PERIOD_MS) {
      console.debug(
        `server-probe: current server down for ${Math.round(unreachableForMs / 1000)}s (< ${GRACE_PERIOD_MS / 1000}s grace), likely a self-restart; staying`,
      );
      return false;
    }
    console.debug(
      `server-probe: current server down for ${Math.round(unreachableForMs / 1000)}s (>= grace), evaluating alternatives (${reason})`,
    );
    const best = await findBestServer(reason);
    if (best && !isCurrentHost(best)) {
      switchToServer(best);
      return true;
    }
    return false;
  }

  return { candidateHosts, findBestServer, switchToServer, reevaluate, notifyServerReachable };
})();
