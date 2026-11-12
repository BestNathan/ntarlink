"""Capture exact host dependency/license provenance for the S2 Linux POC.

This is an observed Ubuntu test baseline, not an immutable OS image.
Binary-version drift fails the controlled POC before it is accepted.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

PACKAGES = ("iproute2", "iputils-ping", "iperf3", "tcpdump")
EXPECTED_CLI = {
    "ip": "iproute2-6.1.0",
    "tc": "iproute2-6.1.0",
    "iperf3": "iperf 3.16",
}


def output(args: list[str]) -> str:
    result = subprocess.run(
        args, capture_output=True, text=True, check=True, timeout=10,
    )
    return (result.stdout or result.stderr).strip()


def collect() -> dict:
    releases = platform.freedesktop_os_release()
    packages = {}
    for name in PACKAGES:
        package = output(["dpkg-query", "-W", "-f=$" + "{Version}", name])
        copyright_path = Path("/usr/share/doc") / name / "copyright"
        if not copyright_path.exists():
            raise RuntimeError(f"missing package copyright text: {copyright_path}")
        packages[name] = {
            "debian_package_version": package,
            "copyright_path": str(copyright_path),
            "copyright_sha256": hashlib.sha256(copyright_path.read_bytes()).hexdigest(),
        }
    versions = {
        "ip": output(["ip", "-Version"]),
        "tc": output(["tc", "-Version"]),
        "iperf3": output(["iperf3", "--version"]).splitlines()[0],
        "tcpdump": output(["tcpdump", "--version"]).splitlines()[0],
        "ping": output(["ping", "-V"]).splitlines()[0],
    }
    return {
        "profile": "ntarlink-S2-host-dependency-evidence-v1",
        "os_id": releases.get("ID"),
        "os_version_id": releases.get("VERSION_ID"),
        "kernel": platform.release(),
        "arch": platform.machine(),
        "github_run_id": os.environ.get("GITHUB_RUN_ID"),
        "git_commit": os.environ.get("GITHUB_SHA"),
        "packages": packages,
        "binary_versions": versions,
        "binary_contract": EXPECTED_CLI,
        "fidelity": "host-toolchain-provenance-not-orbital-hardware",
    }


def validate(manifest: dict) -> None:
    if manifest["os_id"] != "ubuntu" or manifest["os_version_id"] != "24.04":
        raise RuntimeError("S2 POC baseline requires Ubuntu 24.04")
    for binary, expected in EXPECTED_CLI.items():
        if expected not in manifest["binary_versions"][binary]:
            raise RuntimeError(f"{binary} changed: reviewed POC expects {expected!r}")
    for package in PACKAGES:
        if not manifest["packages"][package]["debian_package_version"]:
            raise RuntimeError(f"package {package} has no recorded version")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    manifest = collect()
    if args.check:
        validate(manifest)
    args.output.write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
    print(f"Captured S2 host dependency evidence: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
