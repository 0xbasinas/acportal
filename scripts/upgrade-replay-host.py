#!/usr/bin/env python3
"""Live host for the old-build upgrade replay check (docs/emulator-runbook.md §7).

Starts a real acpd with the mock ACP agent (no model provider, no network beyond
loopback), creates one session, and prints the values the instrumented phases need:
host id, session id and a one-use phone pairing code. Keeps running until Ctrl-C, then
stops the host it started and removes its scratch directory.

The pairing code is printed to this terminal only; do not redirect output to a log.
Uses only the Python standard library.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def http(base, method, path, body=None, token=None):
    request = urllib.request.Request(base + path, method=method, data=None if body is None else json.dumps(body).encode())
    request.add_header("Content-Type", "application/json")
    if token:
        request.add_header("Authorization", "Bearer " + token)
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.loads(response.read() or b"null")


def pairing_code(acpd, config):
    output = subprocess.run([acpd, "pair", "--manual", "--config", config], capture_output=True, text=True, check=True).stdout
    match = re.search(r"Pairing code: (\S+)", output)
    if not match:
        raise SystemExit("acpd pair printed no code")
    return match.group(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--acpd", default="target/debug/acpd")
    parser.add_argument("--mock", default="target/debug/mock-acp-agent")
    parser.add_argument("--listen", default="127.0.0.1:8767", help="use adb reverse tcp:PORT tcp:PORT for the emulator")
    parser.add_argument("--dir", help="scratch directory (default: a new temporary directory, removed on exit)")
    parser.add_argument("--check", action="store_true", help="start, create the session, print, stop (self-test)")
    parser.add_argument("--adb", help="Run the three guarded Android phases without logging pairing codes")
    parser.add_argument("--old-apk")
    parser.add_argument("--current-apk")
    parser.add_argument("--test-apk")
    parser.add_argument("--serial", default="emulator-5554")
    args = parser.parse_args()
    if args.adb and not all((args.old_apk,args.current_apk,args.test_apk)):
        parser.error("Android mode requires old, current and test APKs")
    acpd, mock = os.path.abspath(args.acpd), os.path.abspath(args.mock)
    for binary in (acpd, mock):
        if not os.access(binary, os.X_OK):
            raise SystemExit(f"missing {binary}; run: cargo build -p acpd")
    owned = args.dir is None
    directory = os.path.abspath(args.dir or tempfile.mkdtemp(prefix="acportal-replay-"))
    workspace = os.path.join(directory, "workspace")
    os.makedirs(workspace, exist_ok=True)
    config = os.path.join(directory, "config.toml")
    with open(config, "w") as file:
        file.write(f"registry = 'agents.json'\nstate_directory = 'state'\nworkspace_roots = ['workspace']\n[server]\nlisten = '{args.listen}'\n")
    with open(os.path.join(directory, "agents.json"), "w") as file:
        json.dump([{"id": "mock", "name": "Replay mock", "command": mock, "args": [], "enabled": True}], file)
    base = "http://" + args.listen
    log = open(os.path.join(directory, "acpd-host.out"), "w")
    host = subprocess.Popen([acpd, "--config", config, "start"], stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
    try:
        deadline = time.time() + 20
        while True:
            if host.poll() is not None:
                raise SystemExit(f"acpd exited; see {log.name}")
            try:
                urllib.request.urlopen(base + "/v1/status", timeout=1)
                break
            except urllib.error.HTTPError:
                break  # 401: listening
            except OSError:
                if time.time() > deadline:
                    raise SystemExit("acpd did not start")
                time.sleep(0.2)
        token = http(base, "POST", "/v1/pair", {"code": pairing_code(acpd, config), "deviceName": "upgrade-replay-script"})["token"]
        status = http(base, "GET", "/v1/status", token=token)
        session = http(base, "POST", "/v1/sessions", {"agentId": "mock", "workspace": workspace}, token=token)
        values = {"replayHostId": status["hostId"], "replaySessionId": session["id"], "replayAddress": base}
        if args.adb:
            def adb(*arguments):
                result=subprocess.run([args.adb,"-s",args.serial,*arguments],capture_output=True,text=True,timeout=180)
                if result.returncode:
                    raise RuntimeError("Owned upgrade adb operation failed")
                return result.stdout
            def instrument(class_name,phase_name,*arguments):
                output=adb("shell","am","instrument","-w","-r","-e","class","dev.acportal.data."+class_name,
                    "-e","upgradeReplay",phase_name,*arguments,"dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner")
                if "OK (1 test)" not in output or "FAILURES" in output:
                    raise RuntimeError("Owned upgrade instrumentation failed: "+phase_name)
                print("PASS owned upgrade phase: "+phase_name,flush=True)
            port=args.listen.rsplit(":",1)[1]
            reverse="tcp:"+port
            if reverse in adb("reverse","--list"):
                raise RuntimeError("Fixture reverse port is already owned")
            adb("reverse",reverse,reverse)
            prepared=False
            try:
                adb("install","-r","-d",str(os.path.abspath(args.old_apk)))
                adb("install","-r",str(os.path.abspath(args.test_apk)))
                instrument("UpgradeReplayPreparationTest","prepare","-e","replayHostId",values["replayHostId"],
                    "-e","replaySessionId",values["replaySessionId"],"-e","replayAddress",base)
                prepared=True
                adb("install","-r",str(os.path.abspath(args.current_apk)))
                # Generate only after both installations; it expires after 60 seconds.
                code=pairing_code(acpd,config)
                instrument("UpgradeReplayVerificationTest","verify","-e","replayCode",code)
            finally:
                try:
                    if prepared:
                        instrument("UpgradeReplayCleanupTest","cleanup","-e","replayHostId",values["replayHostId"])
                finally:
                    adb("reverse","--remove",reverse)
            return
        print("Fixture values (pass each with -e):")
        print(json.dumps(values, indent=2))
        if args.check:
            assert session.get("status") in ("ready", "running"), session
            print("self-test ok")
            return
        print("Phone pairing code for the verify phase (one use, keep private):", pairing_code(acpd, config))
        print("Leave this running through prepare, upgrade, verify and cleanup. Ctrl-C stops the host.")
        sys.stdout.flush()
        host.wait()
    except KeyboardInterrupt:
        pass
    finally:
        if host.poll() is None:
            host.terminate()
            try:
                host.wait(timeout=10)
            except subprocess.TimeoutExpired:
                host.kill()
                host.wait()
        log.close()
        if owned:
            shutil.rmtree(directory, ignore_errors=True)


if __name__ == "__main__":
    main()
