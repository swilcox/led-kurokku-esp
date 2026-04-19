#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "PyYAML>=6.0",
#     "esp-idf-nvs-partition-gen>=0.1.4",
# ]
# ///
"""Provision an ESP32 device with NVS config from a per-device YAML file.

Reads tools/devices/<name>.yaml, generates an NVS partition image via
`nvs_partition_gen` (from the `esp-idf-nvs-partition-gen` package),
then flashes it with `espflash write-bin` to the NVS partition offset.

Dependencies are declared via PEP 723 inline script metadata and resolved
automatically by `uv run`. No manual install step required.

Usage:
    uv run tools/provision.py <device-name> [--port /dev/ttyUSB0]
    # or, since the script is marked executable:
    ./tools/provision.py <device-name> [--port /dev/ttyUSB0]

The device YAML file must define at minimum: device_id, wifi_ssid,
wifi_pass, server_url, tz. Optional keys mirror AppConfig in src/config.rs.
"""

from __future__ import annotations

import argparse
import csv
import io
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml


REPO_ROOT = Path(__file__).resolve().parent.parent
DEVICES_DIR = REPO_ROOT / "tools" / "devices"
PARTITIONS_CSV = REPO_ROOT / "partitions.csv"

# NVS key types must match src/config.rs's get/set calls.
# Type strings are the ones nvs_partition_gen understands.
SCHEMA = {
    "wifi_ssid":   ("wifi_ssid",   "string"),
    "wifi_pass":   ("wifi_pass",   "string"),
    "server_url":  ("server_url",  "string"),
    "device_id":   ("device_id",   "string"),
    "tz":          ("tz",          "string"),
    "syslog_host": ("syslog_host", "string"),
    "format_24h":  ("format_24h",  "u8"),
    "brightness":  ("brightness",  "u8"),
    # Firmware reads this via get_i32 + cast (esp-idf-svc's NVS wrapper
    # doesn't expose u32 directly). Use i32 here to match.
    "poll_ms":     ("poll_ms",     "i32"),
}

REQUIRED = ("wifi_ssid", "wifi_pass", "server_url", "device_id", "tz")


def find_nvs_partition() -> tuple[int, int]:
    """Return (offset, size) of the NVS partition.

    Parses partitions.csv if present, else falls back to the built-in
    two-OTA layout (NVS at 0x9000, 16 KB).
    """
    if PARTITIONS_CSV.is_file():
        with PARTITIONS_CSV.open() as f:
            for row in csv.reader(f):
                if not row or row[0].lstrip().startswith("#"):
                    continue
                parts = [c.strip() for c in row]
                if len(parts) >= 5 and parts[0] == "nvs":
                    return int(parts[3], 0), int(parts[4], 0)
    # Built-in two-OTA default
    return 0x9000, 0x4000


def load_device(name: str) -> dict:
    path = DEVICES_DIR / f"{name}.yaml"
    if not path.is_file():
        sys.exit(f"error: device file not found: {path}")
    with path.open() as f:
        data = yaml.safe_load(f) or {}
    missing = [k for k in REQUIRED if not data.get(k)]
    if missing:
        sys.exit(f"error: {path} missing required keys: {', '.join(missing)}")
    return data


def build_nvs_csv(cfg: dict) -> str:
    """Generate the CSV that nvs_partition_gen consumes."""
    buf = io.StringIO()
    buf.write("key,type,encoding,value\n")
    buf.write("kurokku,namespace,,\n")
    for yaml_key, (nvs_key, nvs_type) in SCHEMA.items():
        if yaml_key not in cfg or cfg[yaml_key] is None:
            continue
        value = cfg[yaml_key]
        if nvs_type == "string":
            encoding = "string"
            value_str = str(value)
        else:
            encoding = nvs_type
            # YAML booleans → 0/1 for u8 (format_24h)
            if isinstance(value, bool):
                value_str = "1" if value else "0"
            else:
                value_str = str(int(value))
        # Quote strings to survive commas/spaces.
        if encoding == "string":
            value_str = value_str.replace('"', '\\"')
            buf.write(f'{nvs_key},data,{encoding},"{value_str}"\n')
        else:
            buf.write(f"{nvs_key},data,{encoding},{value_str}\n")
    return buf.getvalue()


def run(cmd: list[str]) -> None:
    print(f"$ {' '.join(cmd)}")
    subprocess.run(cmd, check=True)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Provision an ESP32 device with NVS config from a per-device YAML file."
    )
    parser.add_argument("device", help="device name (matches tools/devices/<name>.yaml)")
    parser.add_argument("--port", help="serial port (auto-detect if omitted)")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="generate NVS image but skip flashing",
    )
    args = parser.parse_args()

    gen = [sys.executable, "-m", "esp_idf_nvs_partition_gen"]
    if not args.dry_run and not shutil.which("espflash"):
        sys.exit("error: espflash not on PATH")

    cfg = load_device(args.device)
    offset, size = find_nvs_partition()
    print(f"NVS partition: offset=0x{offset:x}, size=0x{size:x} ({size} bytes)")

    csv_body = build_nvs_csv(cfg)
    print("--- NVS CSV ---")
    # Redact passwords when echoing
    for line in csv_body.splitlines():
        if line.startswith("wifi_pass,"):
            print("wifi_pass,data,string,\"***\"")
        else:
            print(line)
    print("---------------")

    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        csv_path = td / "nvs.csv"
        bin_path = td / "nvs.bin"
        csv_path.write_text(csv_body)

        run([*gen, "generate", str(csv_path), str(bin_path), str(size)])

        if args.dry_run:
            kept = REPO_ROOT / "tools" / f"{args.device}.nvs.bin"
            shutil.copy(bin_path, kept)
            print(f"dry-run: wrote {kept}")
            return 0

        cmd = ["espflash", "write-bin"]
        if args.port:
            cmd += ["--port", args.port]
        cmd += [f"0x{offset:x}", str(bin_path)]
        run(cmd)

    print(f"provisioned device: {cfg['device_id']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
