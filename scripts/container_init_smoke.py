#!/usr/bin/env python3
"""Verify an already-built Gateway image using disposable local state and keys."""
import argparse
import json
import secrets
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

CURRENT_PHASE = "setup"


def docker(*args):
    result = subprocess.run(["docker", *args], capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise RuntimeError("Docker operation failed: " + args[0])
    return result.stdout.strip()


def request(base, path, token=None):
    headers = {"Authorization": "Bearer " + token} if token else {}
    query = urllib.request.Request(base + path, headers=headers)
    try:
        with urllib.request.urlopen(query, timeout=4) as result:
            return result.status, result.read(65537)
    except urllib.error.HTTPError as error:
        return error.code, error.read(65537)


def main():
    global CURRENT_PHASE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True)
    args = parser.parse_args()
    name = "gateway-init-" + secrets.token_hex(6)
    token = secrets.token_hex(32)
    with tempfile.TemporaryDirectory(prefix="gateway-init-") as directory:
        env = ["PORT=10000", "GATEWAY_STATE_PATH=/state/gateway_state.json",
               "GATEWAY_PERSISTENCE_MODE=exclusive-local-writer",
               "GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM=false",
               "API_TOKEN=" + token, "TOKEN_TTL_SECONDS=3600",
               "FIAT_WEBHOOK_SECRET=" + secrets.token_hex(32),
               "SETTLEMENT_INGRESS_SECRET=" + secrets.token_hex(32),
               "HMAC_SECRET=" + secrets.token_hex(32),
               "OFFLINE_QUEUE_SECRET=" + secrets.token_hex(32),
               "RAMP_MODE=disabled", "STITCH_MODE=disabled", "OZOW_MODE=disabled",
               "PAPSS_MODE=disabled", "ALCHEMY_PAY_MODE=disabled", "BANXA_MODE=disabled",
               "INFOBIP_MODE=disabled", "BITCOIN_RPC_URL=http://127.0.0.1:9",
               "STACKS_RPC_URL=http://127.0.0.1:9"]
        command = ["run", "--detach", "--name", name, "--publish", "127.0.0.1::10000",
                   "--mount", f"type=bind,src={directory},dst=/state"]
        for entry in env:
            command.extend(["--env", entry])
        try:
            CURRENT_PHASE = "container-start"
            docker(*command, args.image)
            port = docker("port", name, "10000/tcp").rsplit(":", 1)[1]
            base = "http://127.0.0.1:" + port

            def check_initialized():
                global CURRENT_PHASE
                CURRENT_PHASE = "readiness"
                deadline = time.monotonic() + 60
                while True:
                    try:
                        code, body = request(base, "/api/v1/health")
                        if code == 200 and len(body) <= 65536 and json.loads(body).get("status") == "ok":
                            break
                    except (OSError, ValueError):
                        pass
                    if time.monotonic() > deadline:
                        raise RuntimeError("Gateway did not initialize")
                    time.sleep(1)
                CURRENT_PHASE = "authentication"
                for credential in [None, secrets.token_hex(32)]:
                    if request(base, "/api/v1/capabilities", credential)[0] != 401:
                        raise RuntimeError("Unauthorized discovery was not denied")
                code, body = request(base, "/api/v1/capabilities", token)
                if code != 200 or len(body) > 65536 or not isinstance(json.loads(body), dict):
                    raise RuntimeError("Authenticated discovery failed")

            check_initialized()
            CURRENT_PHASE = "exclusive-writer"
            # A second writer must fail rather than silently replace state.
            second = name + "-second"
            try:
                docker(*[second if part == name else part for part in command], args.image)
                deadline = time.monotonic() + 15
                while docker("inspect", "--format", "{{.State.Running}}", second) == "true":
                    if time.monotonic() > deadline:
                        raise RuntimeError("Second state writer was accepted")
                    time.sleep(1)
                if docker("inspect", "--format", "{{.State.ExitCode}}", second) == "0":
                    raise RuntimeError("Second state writer did not fail")
                logs = subprocess.run(["docker", "logs", second], capture_output=True,
                                      text=True, timeout=30)
                if "already owned by another Gateway process" not in logs.stdout + logs.stderr:
                    raise RuntimeError("Second writer failed for an unrelated reason")
            finally:
                subprocess.run(["docker", "rm", "--force", second], capture_output=True, timeout=30)
            CURRENT_PHASE = "restart"
            docker("restart", name)
            # Docker may assign a new host port to an ephemeral binding.
            port = docker("port", name, "10000/tcp").rsplit(":", 1)[1]
            base = "http://127.0.0.1:" + port
            check_initialized()
            print(json.dumps({"result": "pass", "scope": "isolated-container-initialization",
                              "checks": ["hosting-port", "authentication-denial",
                                         "authenticated-discovery", "exclusive-writer", "restart"]}))
        finally:
            subprocess.run(["docker", "rm", "--force", name], capture_output=True, timeout=30)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.TimeoutExpired):
        print(json.dumps({"result": "fail", "phase": CURRENT_PHASE,
                          "scope": "isolated-container-initialization"}))
        raise SystemExit(1)

