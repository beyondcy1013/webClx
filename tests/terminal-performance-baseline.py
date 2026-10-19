#!/usr/bin/env python3
"""Terminal performance baseline entry with strict JSONL and fixture cleanup."""
import argparse
import asyncio
import json
import os
import secrets
import shlex
import statistics
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

from playwright.async_api import async_playwright

BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get("WEBCLX_TEST_CHROMIUM", "/home/third_party/browser-tools/bin/chromium")
OWNER = "browser-qa-terminal-performance-baseline"
SCENARIOS = ("first_connection", "session_switch", "output_switch")
STORAGE_STATE = os.environ.get("WEBCLX_TEST_STORAGE_STATE", "")


class ApiError(RuntimeError):
    def __init__(self, action, response):
        self.action = action
        self.status = response.status
        super().__init__(f"{action} failed ({response.status})")


class CleanupFailed(RuntimeError):
    pass


class MeasurementFailure(RuntimeError):
    pass


async def require_ok(response, action):
    if not response.ok:
        raise ApiError(action, response)
    return response


async def response_json(response, action):
    await require_ok(response, action)
    return await response.json()


def workspace_dir_setting(settings):
    value = settings.get("workspace_dir")
    if not isinstance(value, str) or not value.strip():
        raise RuntimeError("settings response has no workspace_dir")
    return value


def percentile(values, fraction):
    values = sorted(values)
    if not values:
        return None
    return round(values[min(len(values) - 1, int((len(values) - 1) * fraction))], 3)


def summarize(samples):
    valid = [float(item["elapsed_ms"]) for item in samples if item["ok"]]
    return {
        "samples": len(samples),
        "valid_samples": len(valid),
        "failure_rate": round((len(samples) - len(valid)) / len(samples), 4) if samples else 0,
        "p50_ms": percentile(valid, 0.50),
        "p95_ms": percentile(valid, 0.95),
        "p99_ms": percentile(valid, 0.99),
        "mean_ms": round(statistics.mean(valid), 3) if valid else None,
    }


async def load_storage_state(path):
    if not path:
        raise RuntimeError("real measurements require WEBCLX_TEST_STORAGE_STATE")
    state_path = Path(path)
    if not state_path.is_file():
        raise RuntimeError(f"storage state does not exist: {path}")
    state = json.loads(state_path.read_text(encoding="utf-8"))
    if not isinstance(state, dict) or not isinstance(state.get("cookies"), list):
        raise RuntimeError(f"invalid Playwright storage state: {path}")
    return path


async def create_session(request, created, suffix, workspace):
    payload = {
        "path": workspace,
        "origin": "normal",
        "owner_key": f"{OWNER}-{secrets.token_hex(8)}-{suffix}",
    }
    session = await response_json(
        await request.post(f"{BASE_URL}/api/terminal/sessions", data=payload),
        "create terminal session",
    )
    if not session.get("id"):
        raise RuntimeError("create response has no session id")
    created[session["id"]] = payload
    # The create/list API may report an empty relative path for the workspace
    # root. Ownership cleanup is keyed by the unique owner_key, so tolerate the
    # path echo shape and require only owner equality here.
    if session.get("owner_key") != payload["owner_key"]:
        raise RuntimeError(f"session ownership mismatch: {session}")
    return session


async def cleanup(request, created):
    """Delete matching fixtures item by item, collect all failures, then recheck."""
    listing = await response_json(
        await request.get(f"{BASE_URL}/api/terminal/sessions?all=true"),
        "list terminal sessions",
    )
    by_id = {item["id"]: item for item in listing.get("sessions", [])}
    errors = []
    processed_ids = []
    for session_id, expected in created.items():
        current = by_id.get(session_id)
        if current is None:
            processed_ids.append(session_id)
            continue
        # Path echo can be normalized to an empty string for the workspace root;
        # unique owner_key remains the authoritative fixture ownership check.
        if current.get("owner_key") != expected["owner_key"]:
            errors.append({"id": session_id, "reason": "ownership mismatch"})
            continue
        try:
            response = await request.delete(
                f"{BASE_URL}/api/terminal/sessions/{session_id}",
                headers={
                    "X-WebClx-Confirm-Session": session_id,
                    "X-WebClx-Delete-Source": "terminal-performance-baseline",
                },
            )
            if not response.ok:
                errors.append({"id": session_id, "status": response.status})
            else:
                processed_ids.append(session_id)
        except Exception as error:
            errors.append({"id": session_id, "error": str(error)})

    if processed_ids:
        recheck = await response_json(
            await request.get(f"{BASE_URL}/api/terminal/sessions?all=true"),
            "recheck terminal sessions",
        )
        remaining = {item["id"] for item in recheck.get("sessions", [])}
        for session_id in processed_ids:
            if session_id in remaining:
                errors.append({"id": session_id, "reason": "delete did not remove fixture"})
    if errors:
        raise CleanupFailed(f"fixture cleanup failed: {errors}")
    return processed_ids


async def send_unique_output(request, session_id, evidence):
    """Build the command from two arguments so command echo lacks the full evidence."""
    prefix = evidence[: max(1, len(evidence) // 2)]
    suffix = evidence[len(prefix):]
    command = f"printf '%s%s\\n' {shlex.quote(prefix)} {shlex.quote(suffix)}\r"
    await require_ok(
        await request.post(
            f"{BASE_URL}/api/terminal/sessions/{session_id}/input",
            data={"data": command},
        ),
        "send output evidence",
    )
    return evidence


async def send_bounded_output(request, session_id, evidence, repeats=4):
    """Send a bounded series of unique-output requests before the measured switch."""
    await send_unique_output(request, session_id, evidence)
    for _ in range(max(0, repeats - 1)):
        await send_unique_output(request, session_id, evidence)


async def wait_buffer_ready(page, session_id, evidence, timeout_ms=20_000):
    """Require target context/socket, drained writes, mounted grid, and evidence."""
    condition = """({sessionId, evidence}) => {
        const context = activeTerminalContext;
        const term = context?.term;
        const buffer = term?.buffer?.active;
        if (!term || !buffer) return false;
        let evidenceVisible = false;
        for (let row = buffer.viewportY; row < buffer.viewportY + term.rows; row += 1) {
            if ((buffer.getLine(row)?.translateToString(true) || '').includes(evidence)) {
                evidenceVisible = true;
                break;
            }
        }
        return state.activeSessionId === sessionId && context.sessionId === sessionId &&
            context.socket?.readyState === WebSocket.OPEN && context.hasLoadedOutput &&
            !context.initialReplayPending && !context.backlogReplayActive &&
            !context.outputWriteInFlight && term.element?.isConnected &&
            term.cols > 0 && term.rows > 0 && evidenceVisible;
    }"""
    await page.wait_for_function(
        condition,
        arg={"sessionId": session_id, "evidence": evidence},
        timeout=timeout_ms,
    )


async def emit_summaries(results, args, output):
    for name in SCENARIOS:
        output({
            "type": "summary",
            "scenario": name,
            "cache": args.cache,
            "cache_scope": "fresh_browser_per_cold_sample",
            "os_cache_controlled": False,
            "metric_scope": "buffer_ready",
            "clock": "monotonic",
            **summarize(results[name]),
        }, sort_keys=True)


async def open_measured_resources(args, browser_factory):
    """Create one independent measured browser/context/fixtures set."""
    browser = await browser_factory(args)
    context = None
    created = {}
    request = None
    try:
        context = await browser.new_context(
            viewport={"width": 1440, "height": 900},
            storage_state=await load_storage_state(args.storage_state),
        )
        request = context.request
        page = await context.new_page()
        settings = await response_json(
            await request.get(f"{BASE_URL}/api/settings"),
            "load settings",
        )
        workspace = workspace_dir_setting(settings)
        first = await create_session(request, created, "a", workspace)
        second = await create_session(request, created, "b", workspace)
        return {
            "browser": browser,
            "context": context,
            "request": request,
            "page": page,
            "created": created,
            "first": first,
            "second": second,
        }
    except Exception:
        cleanup_error = None
        if request is not None:
            try:
                await cleanup(request, created)
            except Exception as error:
                cleanup_error = error
        if context is not None:
            try:
                await context.close()
            except Exception as error:
                if cleanup_error is None:
                    cleanup_error = error
        try:
            await browser.close()
        except Exception as error:
            if cleanup_error is None:
                cleanup_error = error
        if cleanup_error is not None:
            raise cleanup_error
        raise

async def release_measured_resources(resources, cleanup_fixtures=True):
    """Clean fixtures once, then independently close context and browser."""
    errors = []
    if cleanup_fixtures:
        try:
            await cleanup(resources["request"], resources["created"])
        except Exception as error:
            errors.append(error)
    try:
        await resources["request"].dispose()
    except Exception as error:
        errors.append(error)
    try:
        await resources["context"].close()
    except Exception as error:
        errors.append(error)
    try:
        await resources["browser"].close()
    except Exception as error:
        errors.append(error)
    if errors:
        raise errors[0] from (errors[1] if len(errors) > 1 else None)

async def run(args, browser_factory, output=json.dumps):
    """Emit one JSONL sample immediately; emit exactly one summary per scenario."""
    results = {name: [] for name in SCENARIOS}
    sample_count = 0
    summaries_emitted = False
    resources = None
    completed = False
    try:
        resources = await open_measured_resources(args, browser_factory)
        request = resources["request"]
        page = resources["page"]
        first = resources["first"]
        second = resources["second"]
        async def record(name, action):
            nonlocal sample_count
            started = time.monotonic()
            try:
                await action()
                sample = {"scenario": name, "ok": True,
                          "elapsed_ms": round((time.monotonic() - started) * 1000, 3)}
            except Exception as error:
                sample = {"scenario": name, "ok": False,
                          "elapsed_ms": round((time.monotonic() - started) * 1000, 3),
                          "error": str(error)}
            results[name].append(sample)
            sample_count += 1
            output({
                "type": "sample",
                "sample": sample_count,
                "scenario_index": len(results[name]),
                "cache": args.cache,
                "cache_scope": "fresh_browser_per_cold_sample",
                "os_cache_controlled": False,
                "metric_scope": "buffer_ready",
                "clock": "monotonic",
                **sample,
            }, sort_keys=True)
            if not sample["ok"]:
                raise MeasurementFailure(sample["error"])

        # Warm does real navigation/selection for each target and waits for that evidence.
        warm_cycles = args.warmup_samples if args.cache == "warm" else 0
        for _ in range(warm_cycles):
            first_evidence = f"warmup-first-{secrets.token_hex(6)}"
            await send_unique_output(request, first["id"], first_evidence)
            await page.goto(f"{BASE_URL}/terminal?session={first['id']}", wait_until="domcontentloaded")
            await wait_buffer_ready(page, first["id"], first_evidence)
            second_evidence = f"warmup-second-{secrets.token_hex(6)}"
            await send_unique_output(request, second["id"], second_evidence)
            await page.select_option("#session-switcher", second["id"])
            await wait_buffer_ready(page, second["id"], second_evidence)

        for sample_index in range(args.samples):
            if args.cache == "cold" and sample_index > 0:
                # Detach first: release failure must never leave old set to be released again.
                old_resources = resources
                resources = None
                await release_measured_resources(old_resources)
                resources = await open_measured_resources(args, browser_factory)
                request = resources["request"]
                page = resources["page"]
                first = resources["first"]
                second = resources["second"]
            first_evidence = f"baseline-first-{secrets.token_hex(6)}"
            await send_unique_output(request, first["id"], first_evidence)

            async def first_connection():
                await page.goto(
                    f"{BASE_URL}/terminal?session={first['id']}",
                    wait_until="domcontentloaded",
                )
                await wait_buffer_ready(page, first["id"], first_evidence)

            await record("first_connection", first_connection)

            second_evidence = f"baseline-switch-{secrets.token_hex(6)}"
            await send_unique_output(request, second["id"], second_evidence)

            async def session_switch():
                await page.select_option("#session-switcher", second["id"])
                await wait_buffer_ready(page, second["id"], second_evidence)

            await record("session_switch", session_switch)

            # Return to first and wait for first evidence, proving the actual return.
            await page.select_option("#session-switcher", first["id"])
            await wait_buffer_ready(page, first["id"], first_evidence)
            output_evidence = f"baseline-output-{secrets.token_hex(6)}"
            await send_bounded_output(request, second["id"], output_evidence, repeats=4)

            async def output_switch():
                await page.select_option("#session-switcher", second["id"])
                await wait_buffer_ready(page, second["id"], output_evidence)

            await record("output_switch", output_switch)

        await emit_summaries(results, args, output)
        summaries_emitted = True
        completed = True
        return {"results": results, "resources": resources}
    finally:
        sample_error = sys.exc_info()[1] if not completed else None
        if resources is not None:
            try:
                await release_measured_resources(resources)
            except Exception as error:
                cleanup_error = error
            else:
                cleanup_error = None
        else:
            cleanup_error = None
        if not summaries_emitted:
            try:
                await emit_summaries(results, args, output)
            except Exception:
                pass
        if cleanup_error is not None:
            raise cleanup_error from sample_error
        if sample_error is not None:
            raise sample_error


def describe_environment():
    return {
        "base_url": BASE_URL,
        "chromium": CHROMIUM,
        "storage_state_configured": bool(STORAGE_STATE),
        "workspace_source": "GET /api/settings workspace_dir",
        "auth_scope": "product-login storage state shared by browser context and context.request",
        "cache_scope": "fresh_browser_per_cold_sample",
        "cold_preparation": "every recorded cold sample gets a new browser/context and fresh fixtures",
        "warm_preparation": "one browser/context with real first/second navigation and selection warmup",
        "os_cache_controlled": False,
        "output_command": "split printf arguments avoid a full marker in the echoed command",
        "output_scenario": "bounded unique output requests are sent before switching to second",
        "ready_condition": "target context/socket, drained replay/write, mounted xterm grid, and unique evidence in visible buffer",
        "visibility_boundary": "buffer-ready is measured; canvas/pixel rendering is not separately verified",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument("--warmup-samples", type=int, default=3)
    parser.add_argument("--cache", choices=("cold", "warm"))
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    args.storage_state = os.environ.get("WEBCLX_TEST_STORAGE_STATE", "")
    if not args.dry_run and not args.storage_state:
        print("baseline failed: real measurements require WEBCLX_TEST_STORAGE_STATE", file=sys.stderr)
        return 1
    if args.samples < 1:
        parser.error("--samples must be positive")
    if args.warmup_samples < 0:
        parser.error("--warmup-samples cannot be negative")
    if not args.dry_run and args.cache is None:
        parser.error("real measurements require --cache cold or --cache warm")
    if args.dry_run:
        print(json.dumps({"dry_run": True, "scenarios": SCENARIOS, "samples": args.samples,
                          "warmup_samples": args.warmup_samples, "cache": args.cache,
                          "output": "stdout JSONL summaries", **describe_environment()}, sort_keys=True))
        return 0

    async def execute():
        async with async_playwright() as playwright:
            options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
            if Path(CHROMIUM).is_file():
                options["executable_path"] = CHROMIUM

            async def browser_factory(_args):
                return await playwright.chromium.launch(**options)

            await run(args, browser_factory,
                      output=lambda payload, **kw: print(json.dumps(payload, sort_keys=True)))

    try:
        asyncio.run(execute())
    except Exception as error:
        print(f"baseline failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
