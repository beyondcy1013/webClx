#!/usr/bin/env python3
"""Send a tagged message to a webClx terminal."""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


DEFAULT_BASE_URL = "http://127.0.0.1:11111"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Send a tagged message to a webClx terminal.")
    parser.add_argument("--target", default="", help="Target terminal name or session id.")
    parser.add_argument(
        "--agent",
        choices=("codex", "claude"),
        default="",
        help="Discover a terminal running this agent and verify it before delivery.",
    )
    parser.add_argument(
        "--start-if-needed",
        action="store_true",
        help="Start --agent in the resolved terminal when no agent is running there.",
    )
    parser.add_argument(
        "--agent-start-timeout",
        type=float,
        default=30.0,
        help="Seconds to wait for a started agent to appear in the sessions API.",
    )
    parser.add_argument("--message", required=True, help="Message text to send.")
    parser.add_argument("--from", dest="sender", default="", help="Sender terminal name.")
    parser.add_argument("--path", default="", help="Optional target terminal relative path.")
    parser.add_argument(
        "--base-url",
        default=os.environ.get("WEBCLX_URL", DEFAULT_BASE_URL),
        help="webClx base URL. Defaults to WEBCLX_URL or http://127.0.0.1:11111.",
    )
    parser.add_argument(
        "--reply-base-url",
        default=os.environ.get("WEBCLX_REPLY_URL", ""),
        help="Sender webClx URL that the recipient must use when replying.",
    )
    parser.add_argument(
        "--no-enter",
        action="store_true",
        help="Insert text without submitting Enter.",
    )
    parser.add_argument(
        "--submit-enters",
        type=int,
        default=1,
        help="Initial Enter keys to send. Defaults to 1; verified delivery retries Enter when needed.",
    )
    parser.add_argument(
        "--no-verify",
        action="store_true",
        help="Do not require Codex/Claude rollout confirmation (for plain shell targets).",
    )
    parser.add_argument(
        "--raw",
        action="store_true",
        help="Do not add the [from ...] sender prefix.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the JSON payload without sending it.",
    )
    parser.add_argument(
        "--wait-ready-timeout",
        type=float,
        default=120.0,
        help="Seconds to wait when --wait-ready is enabled.",
    )
    parser.add_argument(
        "--wait-ready",
        action="store_true",
        help="Wait for the target terminal to stop being busy before sending.",
    )
    parser.add_argument(
        "--no-wait-ready",
        action="store_true",
        help="Deprecated compatibility flag; immediate sending is now the default.",
    )
    parser.add_argument(
        "--request-reply",
        action="store_true",
        help="Append an instruction telling the recipient to reply with this skill.",
    )
    return parser.parse_args()


def api_json(base_url: str, path: str, payload: dict | None = None) -> dict:
    url = urllib.parse.urljoin(base_url.rstrip("/") + "/", path.lstrip("/"))
    data = None
    headers = {}
    method = "GET"
    if payload is not None:
        data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        headers["Content-Type"] = "application/json"
        method = "POST"
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=45) as response:
            body = response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{error.code} {error.reason}: {detail}") from error
    return json.loads(body)


def normalized_path(value: str) -> str:
    return "/".join(part for part in value.strip().strip("/").split("/") if part)


def normalized_base_url(value: str, option: str) -> str:
    url = value.strip().rstrip("/")
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        raise RuntimeError(f"{option} must be an http(s) URL with a hostname")
    return url


def is_loopback_base_url(base_url: str) -> bool:
    hostname = (urllib.parse.urlparse(base_url).hostname or "").lower()
    return hostname in {"127.0.0.1", "::1", "localhost"}


def safe_single_line(value: str, label: str) -> str:
    cleaned = value.replace("\x00", "").replace("\r\n", "\n").replace("\r", "\n")
    normalized = " ".join(part.strip() for part in cleaned.split("\n") if part.strip())
    if not normalized:
        raise RuntimeError(f"{label} is empty after single-line normalization")
    return normalized


def current_paths() -> set[str]:
    paths = set()
    for raw in (os.environ.get("PWD", ""), os.getcwd()):
        if not raw:
            continue
        path = Path(raw).expanduser()
        paths.add(str(path))
        try:
            paths.add(str(path.resolve()))
        except OSError:
            pass
    return paths


def infer_sender(base_url: str) -> str:
    sessions = api_json(base_url, "/api/terminal/sessions?all=true").get("sessions", [])
    cwd_paths = current_paths()
    matches = []
    for session in sessions:
        display_path = str(session.get("display_path") or "")
        try:
            resolved_display = str(Path(display_path).resolve()) if display_path else ""
        except OSError:
            resolved_display = display_path
        if display_path in cwd_paths or resolved_display in cwd_paths:
            name = str(session.get("name") or "").strip()
            session_id = str(session.get("id") or "").strip()
            if name:
                matches.append((name, session_id))

    unique = []
    seen = set()
    for item in matches:
        if item in seen:
            continue
        seen.add(item)
        unique.append(item)

    if len(unique) == 1:
        return unique[0][0]
    if not unique:
        raise RuntimeError("cannot infer sender terminal; pass --from <terminal-name>")
    candidates = ", ".join(f"{name} ({session_id})" for name, session_id in unique)
    raise RuntimeError(f"sender terminal is ambiguous: {candidates}; pass --from")


def target_matches(session: dict, target: str, path: str) -> bool:
    if str(session.get("id") or "") == target or str(session.get("name") or "") == target:
        if not path:
            return True
        return normalized_path(str(session.get("path") or "")) == path
    return False


def session_path_matches(session: dict, path: str) -> bool:
    if not path:
        return True
    session_path = normalized_path(str(session.get("path") or ""))
    display_path = normalized_path(str(session.get("display_path") or ""))
    return session_path == path or display_path == path or display_path.endswith(f"/{path}")


def session_agent(session: dict) -> str:
    return str(session.get("activity_agent") or "").strip().lower()


def session_label(session: dict) -> str:
    name = str(session.get("name") or "?")
    session_id = str(session.get("id") or "?")
    path = str(session.get("path") or "")
    agent = session_agent(session) or "shell"
    return f"{name} ({session_id}, path={path or '-'}, agent={agent})"


def unique_session(candidates: list[dict], purpose: str) -> dict | None:
    if len(candidates) == 1:
        return candidates[0]
    if len(candidates) > 1:
        labels = ", ".join(session_label(session) for session in candidates)
        raise RuntimeError(f"multiple terminal candidates for {purpose}: {labels}; pass --target or --path")
    return None


def list_sessions(base_url: str) -> list[dict]:
    return api_json(base_url, "/api/terminal/sessions?all=true").get("sessions", [])


def verify_reply_terminal(reply_base_url: str, sender: str) -> None:
    matches = [
        session
        for session in list_sessions(reply_base_url)
        if str(session.get("id") or "") == sender
        or str(session.get("name") or "") == sender
    ]
    selected = unique_session(matches, f"reply terminal {sender} at {reply_base_url}")
    if selected is None:
        raise RuntimeError(
            f"reply endpoint {reply_base_url} cannot resolve sender terminal {sender}"
        )


def resolve_delivery_session(
    base_url: str,
    target: str,
    path: str,
    agent: str,
    start_if_needed: bool,
) -> dict:
    sessions = list_sessions(base_url)
    scoped = [session for session in sessions if session_path_matches(session, path)]

    if target:
        matches = [session for session in scoped if target_matches(session, target, path)]
        selected = unique_session(matches, target)
        if selected is None:
            raise RuntimeError(f"target terminal not found: {target}")
        return selected

    if not agent:
        raise RuntimeError("pass --target or --agent")

    running = [session for session in scoped if session_agent(session) == agent]
    selected = unique_session(running, f"running {agent}")
    if selected is not None:
        return selected
    if not start_if_needed:
        raise RuntimeError(f"no terminal running {agent}; pass --target and --start-if-needed")

    launchable = [
        session
        for session in scoped
        if bool(session.get("connected")) and not session_agent(session)
    ]
    selected = unique_session(launchable, f"starting {agent}")
    if selected is None:
        raise RuntimeError(f"no connected shell terminal available to start {agent}")
    return selected


def start_agent(base_url: str, session: dict, agent: str) -> None:
    api_json(
        base_url,
        "/api/terminal/auto-typed-input",
        {"session_id": str(session.get("id") or ""), "command_line": agent},
    )


def wait_for_agent(base_url: str, session_id: str, agent: str, timeout: float) -> dict:
    deadline = time.monotonic() + max(0.0, timeout)
    while True:
        sessions = list_sessions(base_url)
        session = next(
            (item for item in sessions if str(item.get("id") or "") == session_id),
            None,
        )
        if session is None:
            raise RuntimeError(f"terminal disappeared while starting {agent}: {session_id}")
        detected = session_agent(session)
        if detected == agent:
            return session
        if detected and detected != agent:
            raise RuntimeError(f"terminal {session_label(session)} started a different agent")
        if time.monotonic() >= deadline:
            raise RuntimeError(f"timed out waiting for {agent} in {session_label(session)}")
        time.sleep(2.0)


def resolve_target_session(base_url: str, target: str, path: str) -> dict | None:
    sessions = list_sessions(base_url)
    matches = [session for session in sessions if target_matches(session, target, path)]
    if len(matches) == 1:
        return matches[0]
    return None


def session_ready(session: dict) -> bool:
    if bool(session.get("busy")):
        return False
    return str(session.get("activity_state") or "").strip().lower() != "agent"


def wait_target_ready(base_url: str, target: str, path: str, timeout: float) -> None:
    deadline = time.monotonic() + max(0.0, timeout)
    last_session: dict | None = None
    while True:
        session = resolve_target_session(base_url, target, path)
        if session is None:
            return
        last_session = session
        if session_ready(session):
            return
        if time.monotonic() >= deadline:
            name = session.get("name") or target
            state = session.get("activity_state") or "unknown"
            label = session.get("activity_label") or ""
            raise RuntimeError(f"target {name} is still busy ({state} {label}); retry later or pass --no-wait-ready")
        time.sleep(2.0)


def main() -> int:
    args = parse_args()
    base_url = normalized_base_url(args.base_url, "--base-url")
    path = normalized_path(args.path)
    if args.start_if_needed and not args.agent:
        raise RuntimeError("--start-if-needed requires --agent codex|claude")
    target = args.target.strip()
    if args.agent or not target:
        selected = resolve_delivery_session(
            base_url,
            target,
            path,
            args.agent,
            args.start_if_needed,
        )
        detected_agent = session_agent(selected)
        if args.agent and detected_agent and detected_agent != args.agent:
            raise RuntimeError(
                f"target {session_label(selected)} is running {detected_agent}, not {args.agent}"
            )
        if args.agent and not detected_agent:
            if not args.start_if_needed:
                raise RuntimeError(
                    f"target {session_label(selected)} is not running {args.agent}; pass --start-if-needed"
                )
            start_agent(base_url, selected, args.agent)
            selected = wait_for_agent(
                base_url,
                str(selected.get("id") or ""),
                args.agent,
                args.agent_start_timeout,
            )
        target = str(selected.get("id") or "")

    reply_base_url = ""
    if args.request_reply:
        reply_base_url = args.reply_base_url.strip()
        if not reply_base_url and is_loopback_base_url(base_url):
            reply_base_url = base_url
        if not reply_base_url:
            raise RuntimeError(
                "--request-reply to a remote webClx requires --reply-base-url "
                "or WEBCLX_REPLY_URL so the recipient knows where to reply"
            )
        reply_base_url = normalized_base_url(reply_base_url, "--reply-base-url")
        if not is_loopback_base_url(base_url) and is_loopback_base_url(reply_base_url):
            raise RuntimeError(
                "a remote destination cannot reply to a loopback --reply-base-url; "
                "provide the sender's reachable LAN or public webClx URL"
            )

    sender = args.sender.strip() or os.environ.get("WEBCLX_TERMINAL_NAME", "").strip()
    if not sender and not args.raw:
        sender = infer_sender(reply_base_url or base_url)
    if sender:
        sender = safe_single_line(sender, "sender terminal")

    message = args.message if args.no_enter else safe_single_line(args.message, "message")
    if args.request_reply:
        if args.raw:
            raise RuntimeError("--request-reply requires sender tagging; remove --raw")
        verify_reply_terminal(reply_base_url, sender)
        message = (
            f"{message}；请使用名为 terminal-message 的 skill 回复，"
            f"回复端点为 {reply_base_url}，目标终端为 {sender}，"
            f"不要只在你自己的终端里回答。"
        )
    data = message if args.raw else f"[from {sender}] {message}"
    submit_enters = 0 if args.no_enter else max(1, min(args.submit_enters, 4))
    verify_submission = not args.no_enter and not args.no_verify
    payload = {
        "target": target,
        "data": data,
        "submit": not args.no_enter,
        "submit_enters": submit_enters,
        "bracketed_paste": not args.no_enter,
        "verify_submission": verify_submission,
        "delivery_id": data if verify_submission else "",
    }
    if path:
        payload["path"] = path

    if args.dry_run:
        print(json.dumps(payload, ensure_ascii=False))
        return 0

    if args.wait_ready and not args.no_wait_ready:
        wait_target_ready(base_url, args.target, path, args.wait_ready_timeout)

    response = api_json(base_url, "/api/terminal/sessions/message", payload)
    if verify_submission and not bool(response.get("submitted")):
        detail = json.dumps(response, ensure_ascii=False)
        raise RuntimeError(f"terminal message submission was not confirmed: {detail}")
    print(json.dumps(response, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"send_terminal_message.py: {error}", file=sys.stderr)
        raise SystemExit(1)
