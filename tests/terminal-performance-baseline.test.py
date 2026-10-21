#!/usr/bin/env python3
"""Offline behavior tests with every Playwright boundary mocked."""
import asyncio
import contextlib
import importlib.util
import inspect
import io
import json
import sys
import tempfile
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "tests" / "terminal-performance-baseline.py"
spec = importlib.util.spec_from_file_location("terminal_performance_baseline", MODULE_PATH)
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class ScriptFailure(AssertionError):
    pass


class FakeResponse:
    def __init__(self, payload=None, status=200):
        self.payload = payload if payload is not None else {}
        self.status = status

    @property
    def ok(self):
        return 200 <= self.status < 300

    async def json(self):
        return self.payload


class FakeRequest:
    def __init__(self, routes=None, delete_errors=()):
        self.routes = routes or {}
        self.calls = []
        self.deleted = []
        self.disposed = False
        self.disposed_calls = 0
        self.delete_errors = list(delete_errors)

    async def _call(self, method, url, **kwargs):
        if self.disposed:
            raise RuntimeError("request disposed")
        self.calls.append((method, url, kwargs))
        handler = self.routes.get((method, url), self.routes.get((method, "*")))
        if handler is not None:
            result = handler(self, method, url, **kwargs)
            if hasattr(result, "__await__"):
                return await result
            return result
        if method == "POST" and url.endswith("/api/terminal/sessions"):
            payload = kwargs.get("data") or {}
            return FakeResponse({
                "id": f"s{len(self.calls)}",
                "path": payload.get("path"),
                "owner_key": payload.get("owner_key"),
            })
        if method == "DELETE":
            if self.delete_errors:
                raise self.delete_errors.pop(0)
            self.deleted.append(url.rsplit("/", 1)[-1])
            return FakeResponse({"ok": True})
        return FakeResponse()

    def get(self, url):
        return self._call("GET", url)

    def post(self, url, data=None):
        return self._call("POST", url, data=data)

    def delete(self, url, headers=None):
        return self._call("DELETE", url, headers=headers)

    async def dispose(self):
        if self.disposed:
            raise RuntimeError("request disposed")
        self.disposed_calls += 1
        self.disposed = True


class StatefulPage:
    """Maintains navigation, selected session, output evidence, and failure modes."""

    def __init__(self, sessions, fail_select=None, fail_goto=None, fail_wait=None):
        self.sessions = sessions
        self.selected = None
        self.evidence = {}
        self.selects = []
        self.gotos = []
        self.wait_calls = []
        self.fail_select = fail_select
        self.fail_goto = fail_goto
        self.fail_wait = fail_wait

    async def goto(self, url, **kwargs):
        self.gotos.append(url)
        if self.fail_goto:
            raise self.fail_goto
        marker = "session="
        if marker in url:
            session_id = url.split(marker, 1)[1].split("&", 1)[0]
            self.selected = session_id

    async def select_option(self, selector, value):
        self.selects.append((selector, value))
        if self.fail_select and value == self.fail_select:
            raise RuntimeError(f"select failed: {value}")
        self.selected = value

    async def wait_for_function(self, condition, arg=None, timeout=None):
        state = dict(arg or {})
        self.wait_calls.append(state)
        if self.fail_wait:
            raise self.fail_wait
        session_id = state.get("sessionId")
        evidence = state.get("evidence")
        if self.selected != session_id:
            raise RuntimeError(f"target mismatch: selected={self.selected}, target={session_id}")
        if self.evidence.get(session_id) != evidence:
            raise RuntimeError(f"evidence not sent: {evidence}")
        return True

    def add_output(self, session_id, evidence):
        self.evidence[session_id] = evidence


class FakeContext:
    def __init__(self, request, page, close_error=None):
        self.request = request
        self.page = page
        self.close_error = close_error
        self.closed = False
        self.close_calls = 0

    async def new_page(self):
        return self.page

    async def close(self):
        if self.closed:
            raise RuntimeError("context closed")
        self.close_calls += 1
        if self.close_error:
            raise self.close_error
        self.closed = True


class FakeBrowser:
    def __init__(self, context, close_error=None):
        self.context = context
        self.close_error = close_error
        self.closed = False
        self.close_calls = 0
        self.context_count = 0

    async def new_context(self, **kwargs):
        self.context_count += 1
        self.context.storage_state = kwargs.get("storage_state")
        return self.context

    async def close(self):
        if self.closed:
            raise RuntimeError("browser closed")
        self.close_calls += 1
        if self.close_error:
            raise self.close_error
        self.closed = True


def make_args(samples=1, warmup=0, cache="cold"):
    return type("Args", (), {
        "storage_state": "/tmp/unused",
        "samples": samples,
        "warmup_samples": warmup,
        "cache": cache,
    })()


def storage_state_file(tmp, valid=True):
    path = Path(tmp) / "storage.json"
    path.write_text(json.dumps({"cookies": [], "origins": []} if valid else {}),
                    encoding="utf-8")
    return str(path)


def request_routes(delete_errors=None):
    return {
        ("GET", f"{baseline.BASE_URL}/api/settings"): lambda req, m, u, **kw: FakeResponse(
            {"workspace_dir": "/tmp/workspace"}),
        ("GET", f"{baseline.BASE_URL}/api/terminal/sessions?all=true"): lambda req, m, u, **kw: FakeResponse(
            {"sessions": []}),
    }


async def build_sample_fixture(page_fail_select=None, delete_errors=None):
    request = FakeRequest(request_routes(), delete_errors=delete_errors)
    sessions = {}
    page = StatefulPage(sessions, fail_select=page_fail_select)

    def create_response(req, method, url, **kwargs):
        payload = kwargs["data"]
        session_id = f"s{len(req.calls)}"
        sessions[session_id] = payload["owner_key"]
        return FakeResponse({"id": session_id, "path": payload["path"],
                             "owner_key": payload["owner_key"]})
    request.routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = create_response

    def original_input(req, method, url, **kwargs):
        return FakeResponse({"ok": True})
    request.routes[("POST", "*")] = original_input
    return request, page, FakeContext(request, page), FakeBrowser(FakeContext(request, page))


def connect_page_output_to_requests(page, request):
    """Keep stateful page evidence synchronized with successful input requests."""
    original_post = request.post

    def post(url, data=None):
        if "/input" in url and data and "data" in data:
            command = data["data"]
            if command.startswith("printf '%s%s\\n' "):
                arguments = command[len("printf '%s%s\\n' "):].rstrip("\r").split(" ")
                if len(arguments) != 2:
                    raise RuntimeError(f"bad fake command: {command}")
                evidence = arguments[0].strip("'") + arguments[1].strip("'")
                session_id = url.rsplit("/input", 1)[0].rsplit("/", 1)[-1]
                page.add_output(session_id, evidence)
        return original_post(url, data=data)
    request.post = post


async def run_fixture(args, page=None, request=None, context_close_error=None,
                      browser_close_error=None, output=lambda payload, **kw: None):
    request = request or FakeRequest(request_routes())
    page = page or StatefulPage({})
    context = FakeContext(request, page, close_error=context_close_error)
    browser = FakeBrowser(context, close_error=browser_close_error)
    connect_page_output_to_requests(page, request)
    args.storage_state = storage_state_file(tempfile.mkdtemp())

    async def factory(_args):
        return browser
    await baseline.run(args, factory, output=output)
    return browser, context, request, page


def test_playwright_import_boundary_is_never_invoked_by_behavior_tests():
    source = Path(baseline.__file__).read_text(encoding="utf-8")
    tests = Path(__file__).read_text(encoding="utf-8")
    assert "async_playwright" in source
    assert tests.count("mock.patch.object(baseline, \"async_playwright\"") >= 5
    assert "baseline.main()" in tests
    # Every run test passes a fake factory instead of using production execute().
    assert tests.count("async def factory") >= 3


def test_stdout_jsonl_exactly_three_summaries_and_failure_reason():
    async def scenario():
        emitted = []
        request = FakeRequest(request_routes())
        page = StatefulPage({}, fail_select="s3")
        context = FakeContext(request, page)
        browser = FakeBrowser(context)
        connect_page_output_to_requests(page, request)
        args = make_args()
        args.storage_state = storage_state_file(tempfile.mkdtemp())

        async def factory(_args):
            return browser
        try:
            await baseline.run(args, factory, output=lambda payload, **kw: emitted.append(json.dumps(payload, sort_keys=True)))
        except baseline.MeasurementFailure as error:
            assert "select failed: s3" in str(error)
        else:
            raise ScriptFailure("sample failure did not propagate")
        parsed = [json.loads(line) for line in emitted]
        assert [item["type"] for item in parsed[:2]] == ["sample", "sample"]
        assert parsed[-3:] == [item for item in parsed if item["type"] == "summary"]
        assert len([item for item in parsed if item["type"] == "summary"]) == 3
        assert {item["scenario"] for item in parsed if item["type"] == "summary"} == set(baseline.SCENARIOS)
        assert parsed[0]["scenario"] == "first_connection" and parsed[0]["ok"] is True
        assert parsed[1]["scenario"] == "session_switch"
        assert parsed[1]["ok"] is False and parsed[1]["error"] == "select failed: s3"
        summary = parsed[-2]
        assert summary["scenario"] == "session_switch"
        assert summary["samples"] == 1 and summary["failure_rate"] == 1.0
    asyncio.run(scenario())


def test_stateful_page_requires_navigation_selection_and_exact_evidence():
    async def scenario():
        page = StatefulPage({"s1": "owner"})
        page.selected = "s1"
        page.add_output("s1", "missing")
        try:
            await baseline.wait_buffer_ready(page, "s1", "present", timeout_ms=1)
        except RuntimeError as error:
            assert "evidence not sent" in str(error)
        else:
            raise ScriptFailure("wrong evidence passed")
        page.add_output("s1", "present")
        page.selected = "s2"
        try:
            await baseline.wait_buffer_ready(page, "s1", "present", timeout_ms=1)
        except RuntimeError as error:
            assert "target mismatch" in str(error)
        else:
            raise ScriptFailure("wrong target passed")
        page.selected = "s1"
        await baseline.wait_buffer_ready(page, "s1", "present", timeout_ms=1)
    asyncio.run(scenario())


def test_warm_performs_real_goto_and_select_for_each_target_before_wait():
    async def scenario():
        request = FakeRequest(request_routes())
        sessions = {}
        page = StatefulPage(sessions)
        context = FakeContext(request, page)
        browser = FakeBrowser(context)
        connect_page_output_to_requests(page, request)

        def create_response(req, method, url, **kwargs):
            payload = kwargs["data"]
            session_id = f"s{len(req.calls)}"
            payload = dict(payload)
            payload["id"] = session_id
            sessions[session_id] = payload["owner_key"]
            return FakeResponse({"id": session_id, "path": payload["path"],
                                 "owner_key": payload["owner_key"]})
        request.routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = create_response
        args = make_args(warmup=2, cache="warm")
        args.storage_state = storage_state_file(tempfile.mkdtemp())
        emitted = []

        async def factory(_args):
            return browser
        await baseline.run(args, factory, output=lambda payload, **kw: emitted.append(payload))
        warm_gotos = [url for url in page.gotos if "session=s2" in url]
        second_selects = [item for item in page.selects if item == ("#session-switcher", "s3")]
        warm_waits = [call for call in page.wait_calls if call["evidence"].startswith("warmup-")]
        assert len(warm_gotos) >= 1
        assert len(second_selects) >= 2
        assert len(warm_waits) == 4
        assert [call["sessionId"] for call in warm_waits] == ["s2", "s3", "s2", "s3"]
    asyncio.run(scenario())


def test_output_switch_returns_to_first_then_switches_after_bounded_output():
    async def scenario():
        request = FakeRequest(request_routes())
        sessions = {}
        page = StatefulPage(sessions)
        context = FakeContext(request, page)
        browser = FakeBrowser(context)
        connect_page_output_to_requests(page, request)

        def create_response(req, method, url, **kwargs):
            payload = kwargs["data"]
            session_id = f"s{len(req.calls)}"
            payload = dict(payload)
            payload["id"] = session_id
            sessions[session_id] = payload["owner_key"]
            return FakeResponse({"id": session_id, "path": payload["path"],
                                 "owner_key": payload["owner_key"]})
        request.routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = create_response
        args = make_args(cache="cold")
        args.storage_state = storage_state_file(tempfile.mkdtemp())
        emitted = []

        async def factory(_args):
            return browser
        await baseline.run(args, factory, output=lambda payload, **kw: emitted.append(payload))
        input_positions = [index for index, call in enumerate(request.calls)
                           if call[0] == "POST" and "/input" in call[1]]
        return_position = next(index for index, item in enumerate(page.selects)
                               if item == ("#session-switcher", "s2"))
        return_call_index = next(index for index, call in enumerate(request.calls)
                                 if call[0] == "POST" and "/input" in call[1]
                                 and index > input_positions[2])
        final_output_inputs = input_positions[-4:]
        assert len(final_output_inputs) == 4
        commands = [request.calls[index][2]["data"]["data"] for index in final_output_inputs]
        assert len(set(commands)) == 1
        assert all(command.startswith("printf '%s%s\\n' ") for command in commands)
        arguments = commands[-1][len("printf '%s%s\\n' "):].rstrip("\r").split(" ")
        assert len(arguments) == 2
        full_evidence = arguments[0].strip("'") + arguments[1].strip("'")
        assert full_evidence not in commands[-1]
        assert page.selects[-1] == ("#session-switcher", "s3")
        assert page.wait_calls[-1] == {"sessionId": "s3", "evidence": full_evidence}
    asyncio.run(scenario())


def make_resource_factory(state, fail_second_launch=False, fail_second_context=False,
                          fail_select=None):
    """Factory creates a fresh browser/context each call and refuses closed objects."""
    count = 0

    async def factory(_args):
        nonlocal count
        count += 1
        if count == 2 and fail_second_launch:
            raise RuntimeError("second launch failed")
        request = FakeRequest(request_routes())
        sessions = {}
        page = StatefulPage(sessions, fail_select=fail_select)
        context = FakeContext(request, page)
        browser = FakeBrowser(context)
        connect_page_output_to_requests(page, request)

        def create_response(req, method, url, **kwargs):
            payload = kwargs["data"]
            session_id = f"{count}-{len(req.calls)}"
            sessions[session_id] = payload["owner_key"]
            return FakeResponse({"id": session_id, "path": payload["path"],
                                 "owner_key": payload["owner_key"]})
        request.routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = create_response
        if count == 2 and fail_second_context:
            async def failed_context(**kwargs):
                raise RuntimeError("second context failed")
            browser.new_context = failed_context
        browser.baseline_count = count
        browser.sessions = sessions
        browser.request = request
        browser.page = page
        browser.context = context
        return browser
    return factory, count


async def test_cold_recreates_resources_each_sample_and_records_counts():
    factory, counter = make_resource_factory({})
    args = make_args(samples=2, cache="cold")
    args.storage_state = storage_state_file(tempfile.mkdtemp())
    emitted = []
    browsers = []
    original_run = baseline.run

    async def observed_run(run_args, run_factory, output=json.dumps):
        # Capture each browser object exposed by the factory through a wrapper.
        async def counting_factory(inner_args):
            browser = await run_factory(inner_args)
            browsers.append(browser)
            return browser
        return await original_run(run_args, counting_factory, output=output)
    await observed_run(args, factory, output=lambda payload, **kw: emitted.append(payload))
    assert len(browsers) == 2
    assert len({id(item) for item in browsers}) == 2
    assert all(not item.closed or item.close_calls == 1 for item in browsers)
    assert all(item.close_calls == 1 for item in browsers)
    assert all(item.context.close_calls == 1 for item in browsers)
    assert all(item.request.disposed_calls == 1 for item in browsers)
    assert len({id(item.request) for item in browsers}) == 2
    # Closed context/browser/request must reject further use.
    for item in browsers:
        try:
            await item.context.close()
        except RuntimeError as error:
            assert "context closed" in str(error)
        else:
            raise ScriptFailure("closed context allowed reuse")
        try:
            await item.close()
        except RuntimeError as error:
            assert "browser closed" in str(error)
        else:
            raise ScriptFailure("closed browser allowed reuse")
        try:
            await item.request.get(f"{baseline.BASE_URL}/api/settings")
        except RuntimeError as error:
            assert "request disposed" in str(error)
        else:
            raise ScriptFailure("disposed request allowed reuse")
    create_calls = [call for item in browsers for call in item.request.calls
                    if call[0] == "POST" and call[1].endswith("/api/terminal/sessions")]
    assert len(create_calls) == 4
    summaries = [item for item in emitted if item["type"] == "summary"]
    assert len(summaries) == 3
    assert summaries[0]["cache_scope"] == "fresh_browser_per_cold_sample"


async def test_cold_second_launch_failure_keeps_one_close_and_three_summaries():
    factory, _ = make_resource_factory({}, fail_second_launch=True)
    args = make_args(samples=2, cache="cold")
    args.storage_state = storage_state_file(tempfile.mkdtemp())
    emitted = []
    browsers = []
    original_run = baseline.run

    async def observed_run(run_args, run_factory, output=json.dumps):
        async def counting_factory(inner_args):
            browser = await run_factory(inner_args)
            browsers.append(browser)
            return browser
        return await original_run(run_args, counting_factory, output=output)
    try:
        await observed_run(args, factory, output=lambda payload, **kw: emitted.append(payload))
    except RuntimeError as error:
        assert "second launch failed" in str(error)
    else:
        raise ScriptFailure("second launch failure did not propagate")
    summaries = [item for item in emitted if item["type"] == "summary"]
    assert len(summaries) == 3
    assert all(item.close_calls == 1 for item in browsers)
    assert all(item.context.close_calls == 1 for item in browsers)
    assert all(item.request.disposed_calls == 1 for item in browsers)


async def test_cold_second_context_failure_cleans_first_and_reports_reason():
    factory, _ = make_resource_factory({}, fail_second_context=True)
    args = make_args(samples=2, cache="cold")
    args.storage_state = storage_state_file(tempfile.mkdtemp())
    emitted = []
    browsers = []
    original_run = baseline.run

    async def observed_run(run_args, run_factory, output=json.dumps):
        async def counting_factory(inner_args):
            browser = await run_factory(inner_args)
            browsers.append(browser)
            return browser
        return await original_run(run_args, counting_factory, output=output)
    try:
        await observed_run(args, factory, output=lambda payload, **kw: emitted.append(payload))
    except RuntimeError as error:
        assert "second context failed" in str(error)
    else:
        raise ScriptFailure("second context failure did not propagate")
    summaries = [item for item in emitted if item["type"] == "summary"]
    assert len(summaries) == 3
    assert len(browsers) == 2
    assert browsers[0].close_calls == 1 and browsers[0].context.close_calls == 1
    assert browsers[0].request.disposed_calls == 1
    assert browsers[1].close_calls == 1 and browsers[1].context.close_calls == 0
    assert browsers[1].request.disposed_calls == 0


def test_cold_reset_cleanup_error_still_closes_context_and_browser_without_reuse():
    async def scenario():
        class ListingRequest(FakeRequest):
            async def _call(self, method, url, **kwargs):
                if method == "GET" and url.endswith("/api/terminal/sessions?all=true"):
                    listed = []
                    next_id = 2
                    for call in self.calls:
                        if call[0] == "POST" and call[1].endswith("/api/terminal/sessions"):
                            payload = call[2]["data"]
                            listed.append({"id": f"s{next_id}", "path": payload["path"],
                                           "owner_key": payload["owner_key"]})
                            next_id += 1
                    return FakeResponse({"sessions": listed})
                return await super()._call(method, url, **kwargs)

        request = ListingRequest(request_routes(), delete_errors=[RuntimeError("reset transport")])
        page = StatefulPage({})
        context = FakeContext(request, page)
        browser = FakeBrowser(context)
        connect_page_output_to_requests(page, request)

        def create_response(req, method, url, **kwargs):
            payload = kwargs["data"]
            session_id = f"s{len(req.calls)}"
            return FakeResponse({"id": session_id, "path": payload["path"],
                                 "owner_key": payload["owner_key"]})
        request.routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = create_response
        args = make_args(samples=2, cache="cold")
        args.storage_state = storage_state_file(tempfile.mkdtemp())
        emitted = []

        async def factory(_args):
            return browser
        try:
            await baseline.run(args, factory, output=lambda payload, **kw: emitted.append(payload))
        except baseline.CleanupFailed as error:
            assert "reset transport" in str(error) or "delete did not remove fixture" in str(error)
        else:
            raise ScriptFailure("cold reset cleanup failure did not propagate")
        assert browser.close_calls >= 1
        assert context.close_calls >= 1
        summaries = [item for item in emitted if item["type"] == "summary"]
        assert len(summaries) == 3
    asyncio.run(scenario())


def test_cleanup_transport_failures_attempt_every_delete_and_recheck():
    async def scenario():
        errors = [RuntimeError("transport one"), RuntimeError("transport two")]
        request = FakeRequest(request_routes(), delete_errors=errors)
        listing = [
            {"id": "s1", "path": "/tmp/workspace", "owner_key": "owned-a"},
            {"id": "s2", "path": "/tmp/workspace", "owner_key": "owned-b"},
        ]
        request.routes[("GET", f"{baseline.BASE_URL}/api/terminal/sessions?all=true")] = (
            lambda req, m, u, **kw: FakeResponse({"sessions": list(listing)}))
        created = {
            "s1": {"path": "/tmp/workspace", "owner_key": "owned-a"},
            "s2": {"path": "/tmp/workspace", "owner_key": "owned-b"},
        }
        try:
            await baseline.cleanup(request, created)
        except baseline.CleanupFailed as error:
            assert "transport one" in str(error) and "transport two" in str(error)
            delete_calls = [call for call in request.calls if call[0] == "DELETE"]
            assert [call[1].rsplit("/", 1)[-1] for call in delete_calls] == ["s1", "s2"]
            assert sum(call[0] == "GET" for call in request.calls) == 1
        else:
            raise ScriptFailure("transport failures were not reported")
    asyncio.run(scenario())


def test_owner_response_must_match_exactly_and_mismatched_fixture_not_deleted():
    async def scenario():
        routes = request_routes()
        routes[("GET", f"{baseline.BASE_URL}/api/terminal/sessions?all=true")] = (
            lambda req, m, u, **kw: FakeResponse({"sessions": [
                {"id": "s1", "path": "/tmp/workspace", "owner_key": "different"}]}))
        routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = (
            lambda req, m, u, **kw: FakeResponse({"id": "s1", "path": "/tmp/workspace",
                                                   "owner_key": "different"}))
        request = FakeRequest(routes)
        page = StatefulPage({})
        context = FakeContext(request, page)
        browser = FakeBrowser(context)
        args = make_args()
        args.storage_state = storage_state_file(tempfile.mkdtemp())
        emitted = []

        async def factory(_args):
            return browser
        try:
            await baseline.run(args, factory, output=lambda payload, **kw: emitted.append(payload))
        except baseline.CleanupFailed as error:
            assert "ownership mismatch" in str(error)
        else:
            raise ScriptFailure("owner mismatch did not fail")
        assert not any(call[0] == "DELETE" for call in request.calls)
        assert browser.closed is True and context.closed is True

        request2 = FakeRequest()
        request2.routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = (
            lambda req, m, u, **kw: FakeResponse({"id": "created", "path": "/tmp/workspace",
                                                   "owner_key": "different"}))
        created = {}
        try:
            await baseline.create_session(request2, created, "a", "/tmp/workspace")
        except RuntimeError as error:
            assert "session ownership mismatch" in str(error)
            assert "created" in created
        else:
            raise ScriptFailure("strict owner equality not enforced")
    asyncio.run(scenario())


def test_cleanup_runs_before_close_and_close_exceptions_still_continue():
    async def scenario():
        request = FakeRequest(request_routes())
        sessions = {}
        page = StatefulPage(sessions)
        context = FakeContext(request, page, close_error=RuntimeError("context close failed"))
        browser = FakeBrowser(context, close_error=RuntimeError("browser close failed"))
        connect_page_output_to_requests(page, request)

        def create_response(req, method, url, **kwargs):
            payload = kwargs["data"]
            session_id = f"s{len(req.calls)}"
            payload = dict(payload)
            payload["id"] = session_id
            sessions[session_id] = payload["owner_key"]
            return FakeResponse({"id": session_id, "path": payload["path"],
                                 "owner_key": payload["owner_key"]})
        request.routes[("POST", f"{baseline.BASE_URL}/api/terminal/sessions")] = create_response
        args = make_args()
        args.storage_state = storage_state_file(tempfile.mkdtemp())
        emitted = []

        async def factory(_args):
            return browser
        try:
            await baseline.run(args, factory, output=lambda payload, **kw: emitted.append(payload))
        except RuntimeError as error:
            assert "context close failed" in str(error)
        else:
            raise ScriptFailure("close failure did not propagate")
        assert browser.close_calls == 1 and context.close_calls == 1
        # Clean JSONL and exactly three summaries remain despite close failures.
        summaries = [item for item in emitted if item["type"] == "summary"]
        assert len(summaries) == 3
    asyncio.run(scenario())


def test_main_missing_storage_has_exact_reason_without_playwright_launch():
    original_argv = sys.argv
    original_state = baseline.os.environ.get("WEBCLX_TEST_STORAGE_STATE")
    baseline.os.environ.pop("WEBCLX_TEST_STORAGE_STATE", None)
    launch_calls = []

    class ForbiddenPlaywright:
        def __call__(self):
            launch_calls.append(True)
            raise AssertionError("async_playwright must not start for missing storage state")
    try:
        sys.argv = ["terminal-performance-baseline.py", "--samples", "1", "--cache", "cold"]
        stderr = io.StringIO()
        with mock.patch.object(baseline, "async_playwright", ForbiddenPlaywright()):
            with redirect_stderr(stderr):
                rc = baseline.main()
        assert rc == 1
        assert "real measurements require WEBCLX_TEST_STORAGE_STATE" in stderr.getvalue()
        assert launch_calls == []
    finally:
        sys.argv = original_argv
        if original_state is None:
            baseline.os.environ.pop("WEBCLX_TEST_STORAGE_STATE", None)
        else:
            baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = original_state


def test_main_dry_run_stdout_and_help_exit_zero_without_playwright():
    original_argv = sys.argv
    original_state = baseline.os.environ.get("WEBCLX_TEST_STORAGE_STATE")
    baseline.os.environ.pop("WEBCLX_TEST_STORAGE_STATE", None)

    class ForbiddenPlaywright:
        def __call__(self):
            raise AssertionError("dry run must not start async_playwright")
    try:
        sys.argv = ["terminal-performance-baseline.py", "--dry-run", "--samples", "1",
                    "--cache", "warm"]
        stdout = io.StringIO()
        with mock.patch.object(baseline, "async_playwright", ForbiddenPlaywright()):
            with redirect_stdout(stdout):
                assert baseline.main() == 0
        payload = json.loads(stdout.getvalue())
        assert payload["dry_run"] is True
        assert payload["cache_scope"] == "fresh_browser_per_cold_sample"
        sys.argv = ["terminal-performance-baseline.py", "--help"]
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            try:
                baseline.main()
            except SystemExit as exit_error:
                assert exit_error.code == 0
        assert "usage:" in stdout.getvalue()
    finally:
        sys.argv = original_argv
        if original_state is None:
            baseline.os.environ.pop("WEBCLX_TEST_STORAGE_STATE", None)
        else:
            baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = original_state


class FakeChromiumLauncher:
    def __init__(self, browser_factory, fail=False):
        self.browser_factory = browser_factory
        self.fail = fail
        self.calls = 0

    async def launch(self, **kwargs):
        self.calls += 1
        if self.fail:
            raise RuntimeError("main launch failed")
        return await self.browser_factory(None)


class FakePlaywright:
    def __init__(self, chromium):
        self.chromium = chromium

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc, tb):
        return False


def install_main_playwright_mock(fail_launch=False, fail_select=None):
    """Patch the full Playwright async context manager used by real main."""
    state = {"contexts": []}
    factory, _counter = make_resource_factory({}, fail_select=fail_select)

    async def browser_factory(_args):
        return await factory(_args)
    chromium = FakeChromiumLauncher(browser_factory, fail=fail_launch)

    def fake_async_playwright():
        return FakePlaywright(chromium)
    return fake_async_playwright, chromium, state


def test_main_real_path_success_emits_stdout_jsonl_and_exact_summaries():
    fake_playwright, chromium, _state = install_main_playwright_mock()
    original_argv = sys.argv
    original_state = baseline.os.environ.get("WEBCLX_TEST_STORAGE_STATE")
    state_file = storage_state_file(tempfile.mkdtemp())
    baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = state_file
    try:
        sys.argv = ["terminal-performance-baseline.py", "--samples", "1", "--cache", "cold"]
        stdout = io.StringIO()
        with mock.patch.object(baseline, "async_playwright", fake_playwright):
            with redirect_stdout(stdout):
                rc = baseline.main()
        assert rc == 0
        parsed = [json.loads(line) for line in stdout.getvalue().splitlines()]
        assert [item["type"] for item in parsed[:3]] == ["sample", "sample", "sample"]
        assert [item["type"] for item in parsed[-3:]] == ["summary", "summary", "summary"]
        assert len([item for item in parsed if item["type"] == "summary"]) == 3
        assert chromium.calls == 1
    finally:
        sys.argv = original_argv
        if original_state is None:
            baseline.os.environ.pop("WEBCLX_TEST_STORAGE_STATE", None)
        else:
            baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = original_state


def test_main_real_path_sample_failure_has_reason_nonzero_and_summaries():
    """Keep real run; force StatefulPage select failure at second target."""
    fake_playwright, chromium, _state = install_main_playwright_mock(fail_select="1-3")
    original_argv = sys.argv
    original_state = baseline.os.environ.get("WEBCLX_TEST_STORAGE_STATE")
    baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = storage_state_file(tempfile.mkdtemp())
    try:
        sys.argv = ["terminal-performance-baseline.py", "--samples", "1", "--cache", "cold"]
        stdout = io.StringIO()
        stderr = io.StringIO()
        with mock.patch.object(baseline, "async_playwright", fake_playwright), \
             redirect_stderr(stderr):
            with redirect_stdout(stdout), redirect_stderr(stderr):
                rc = baseline.main()
        assert rc == 1
        assert "select failed: 1-3" in stderr.getvalue()
        parsed = [json.loads(line) for line in stdout.getvalue().splitlines()]
        assert len(parsed) == 5
        assert [item["type"] for item in parsed] == ["sample", "sample", "summary", "summary", "summary"]
        assert parsed[1]["type"] == "sample" and parsed[1]["ok"] is False
        assert parsed[1]["error"] == "select failed: 1-3"
        summaries = [item for item in parsed if item["type"] == "summary"]
        assert len(summaries) == 3
    finally:
        sys.argv = original_argv
        if original_state is None:
            baseline.os.environ.pop("WEBCLX_TEST_STORAGE_STATE", None)
        else:
            baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = original_state


def test_main_real_path_launch_failure_is_nonzero_with_reason():
    fake_playwright, chromium, _state = install_main_playwright_mock(fail_launch=True)
    original_argv = sys.argv
    original_state = baseline.os.environ.get("WEBCLX_TEST_STORAGE_STATE")
    baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = storage_state_file(tempfile.mkdtemp())
    try:
        sys.argv = ["terminal-performance-baseline.py", "--samples", "1", "--cache", "cold"]
        stdout = io.StringIO()
        stderr = io.StringIO()
        with mock.patch.object(baseline, "async_playwright", fake_playwright):
            with redirect_stdout(stdout), redirect_stderr(stderr):
                rc = baseline.main()
        assert rc == 1
        assert "main launch failed" in stderr.getvalue()
        launch_stdout = stdout.getvalue()
        parsed_launch = [json.loads(line) for line in launch_stdout.splitlines()]
        assert len(parsed_launch) == 3
        assert all(item["type"] == "summary" for item in parsed_launch)
        assert chromium.calls == 1
    finally:
        sys.argv = original_argv
        if original_state is None:
            baseline.os.environ.pop("WEBCLX_TEST_STORAGE_STATE", None)
        else:
            baseline.os.environ["WEBCLX_TEST_STORAGE_STATE"] = original_state


if __name__ == "__main__":
    tests = [value for name, value in globals().items() if name.startswith("test_")]
    for test in tests:
        if inspect.iscoroutinefunction(test):
            asyncio.run(test())
        else:
            test()
    print(f"terminal performance baseline offline behavior tests passed ({len(tests)})")
