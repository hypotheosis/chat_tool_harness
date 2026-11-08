#!/usr/bin/env python3
"""Real test-owned app process death against a loopback MCP write counter.
Uses existing Flutter/adb only. Never stops an emulator or clears app/key data.
"""
import json
import re
import subprocess
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "com.example.chat_tool_harness"
SCENARIO = str(uuid.uuid4())
PRIVATE = f"no_backup/task12-process-{SCENARIO}"
COUNTS = {"requests": 0, "writes": 0}
LOCK = threading.Lock()
DONE = threading.Event()


def run(*args, check=True):
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)
    if check and result.returncode:
        raise RuntimeError(f"{' '.join(args)}: {result.stdout}\n{result.stderr}")
    return result.stdout.strip()


def adb(*args, check=True):
    return run("adb", "-s", "emulator-5554", *args, check=check)


class Fixture(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        with LOCK:
            COUNTS["requests"] += 1
        if self.path.endswith("/chat/completions"):
            name = next(t["function"]["name"] for t in body["tools"] if t["function"]["description"] == "write")
            delta = {"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "once", "type": "function", "function": {"name": name, "arguments": "{}"}}]}, "finish_reason": "tool_calls"}]}
            payload = f"data: {json.dumps(delta)}\n\ndata: [DONE]\n\n".encode()
            kind, status = "text/event-stream", 200
        else:
            method = body["method"]
            if method == "tools/call":
                with LOCK:
                    COUNTS["writes"] += 1
                # Remote write has occurred, but no response is sent to the app.
                DONE.wait(120)
                return
            if method == "notifications/initialized":
                payload, kind, status = b"", "application/json", 202
            else:
                result = ({"protocolVersion": body["params"]["protocolVersion"], "capabilities": {"tools": {}}, "serverInfo": {"name": "controlled", "version": "1"}}
                          if method == "initialize" else {"tools": [{"name": "write", "description": "test-owned write", "inputSchema": {"type": "object", "properties": {}}}]})
                payload = json.dumps({"jsonrpc": "2.0", "id": body["id"], "result": result}).encode()
                kind, status = "application/json", 200
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def state():
    raw = adb("shell", "run-as", PACKAGE, "cat", f"{PRIVATE}/state.json", check=False)
    try:
        return json.loads(raw)
    except ValueError:
        return {}


def wait_for(phase):
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        value = state()
        if value.get("phase") == "failed":
            raise AssertionError((value, COUNTS))
        if value.get("phase") == phase:
            return value
        time.sleep(0.1)  # OS/device harness polling, not a headless backend test.
    raise TimeoutError((phase, state()))


def kill_owned_process():
    pid = adb("shell", "pidof", PACKAGE)
    if not re.fullmatch(r"[0-9]+", pid):
        raise AssertionError(f"Expected exactly the test-owned main process, got {pid!r}")
    adb("shell", "input", "keyevent", "KEYCODE_HOME")
    adb("shell", "run-as", PACKAGE, "kill", "-9", pid)
    deadline = time.monotonic() + 10
    while adb("shell", "pidof", PACKAGE, check=False) and time.monotonic() < deadline:
        time.sleep(0.05)
    assert not adb("shell", "pidof", PACKAGE, check=False), "Owned process did not die"
    return pid


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    port = server.server_port
    url = f"http://127.0.0.1:{port}"
    installed = False
    try:
        print(run("flutter", "build", "apk", "--debug", "--target", "integration_test/process_recovery_scenario.dart", f"--dart-define=TASK12_SCENARIO={SCENARIO}", f"--dart-define=TASK12_FIXTURE={url}"), flush=True)
        adb("install", "-r", "build/app/outputs/flutter-apk/app-debug.apk")
        installed = True
        adb("reverse", f"tcp:{port}", f"tcp:{port}")
        adb("shell", "am", "start", "-W", "-n", f"{PACKAGE}/.MainActivity")
        first = wait_for("waiting")
        assert COUNTS["writes"] == 0, COUNTS
        before = COUNTS["requests"]
        first_pid = kill_owned_process()
        # Actual PendingIntent Open action routing after process death. Only IDs,
        # never an approval payload, cross this boundary.
        adb("shell", "am", "start", "-W", "-n", f"{PACKAGE}/.MainActivity", "-a", "com.example.chat_tool_harness.OPEN_AGENT", "--es", "runId", first["runId"], "--es", "conversationId", first["conversationId"])
        second = wait_for("awaitGesture")
        assert COUNTS == {"requests": before, "writes": 0}, COUNTS
        assert first["runtimeId"] != second["runtimeId"]
        dimensions = re.search(r"(\d+)x(\d+)", adb("shell", "wm", "size"))
        width, height = map(int, dimensions.groups())
        adb("shell", "input", "tap", str(width // 2), str(height // 2))
        wait_for("dispatched")
        deadline = time.monotonic() + 20
        while COUNTS["writes"] == 0 and time.monotonic() < deadline:
            time.sleep(0.05)
        assert COUNTS["writes"] == 1, COUNTS
        before = COUNTS["requests"]
        second_pid = kill_owned_process()
        assert first_pid != second_pid
        adb("shell", "am", "start", "-W", "-n", f"{PACKAGE}/.MainActivity")
        recovered = wait_for("passed")
        assert recovered["runtimeId"] != second["runtimeId"]
        assert COUNTS == {"requests": before, "writes": 1}, COUNTS
        print(f"PASS: waiting Open-only restart + real foreground tap + sent-write process death; writes=1, no automatic request/replay; PIDs {first_pid}->{second_pid}->{adb('shell', 'pidof', PACKAGE)}", flush=True)
    finally:
        DONE.set()
        server.shutdown()
        server.server_close()
        adb("reverse", "--remove", f"tcp:{port}", check=False)
        if installed:
            # Only this UUID-owned fixture directory; no app clear/uninstall/key deletion.
            if adb("shell", "pidof", PACKAGE, check=False):
                kill_owned_process()
            adb("shell", "run-as", PACKAGE, "rm", "-r", PRIVATE, check=False)
            # Do not leave the standalone fixture entrypoint installed as the app.
            print(run("flutter", "build", "apk", "--debug", "--target", "lib/main.dart"), flush=True)
            adb("install", "-r", "build/app/outputs/flutter-apk/app-debug.apk")


if __name__ == "__main__":
    main()
