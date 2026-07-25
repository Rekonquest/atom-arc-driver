#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
helper="${1:-$root/target/wsl/release/atom-arc-dxcore-helper}"

success="$($helper --vendor 0x8086 --device 0x56a5 --timeout-ms 5000)"
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["schema"] == 1
assert r["passed"] is True
assert r["vendor_id"] == 0x8086
assert r["device_id"] == 0x56A5
assert r["submitted"] is True
assert r["fence_completed"] is True
assert r["resources_created"] is True
assert r["copy_recorded"] is True
assert r["copy_verified"] is True
assert r["bytes_copied"] == 4096
assert r["copy_checksum"] == 522240
assert r["compute_pipeline_created"] is True
assert r["compute_input_uploaded"] is True
assert r["compute_dispatched"] is True
assert r["compute_verified"] is True
assert r["compute_elements"] == 1024
assert r["compute_input_bytes"] == 4096
assert r["compute_output_bytes"] == 4096
expected_compute_checksum = sum(
    ((((index * 1664525) + r["compute_addend"]) & 0xffffffff) ^ 0xa5a5a5a5)
    for index in range(r["compute_elements"])
)
assert r["compute_checksum"] == expected_compute_checksum
assert r["device_removed_reason"] == 0
' <<<"$success"

custom="$($helper --vendor 0x8086 --device 0x56a5 --elements 257 --addend 7 --timeout-ms 5000)"
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["passed"] is True
assert r["compute_verified"] is True
assert r["compute_elements"] == 257
assert r["compute_addend"] == 7
assert r["compute_input_uploaded"] is True
assert r["compute_input_bytes"] == 1028
assert r["compute_output_bytes"] == 1028
expected = sum(
    ((((index * 1664525) + 7) & 0xffffffff) ^ 0xa5a5a5a5)
    for index in range(257)
)
assert r["compute_checksum"] == expected
' <<<"$custom"

transfer="$($helper --transfer-lab --vendor 0x8086 --device 0x56a5 --payload-bytes 4096 --operations 4 --samples 10 --warmups 1 --timeout-ms 5000)"
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["passed"] is True
assert r["backend"] == "wsl-dxcore-d3d12"
assert r["stage"] == "complete"
assert r["mode"] == "paired-transfer-lab"
assert r["primitive"] == "reusable-staging-buffer-pool"
assert r["vendor_id"] == 0x8086
assert r["device_id"] == 0x56A5
assert r["payload_bytes"] == 4096
assert r["operations_per_sample"] == 4
assert r["samples"] == 10
assert r["warmups"] == 1
assert r["pool_setup_ns"] > 0
assert len(r["baseline_durations_ns"]) == 10
assert len(r["candidate_durations_ns"]) == 10
assert len(r["correctness_digests"]) == 10
assert all(value > 0 for value in r["baseline_durations_ns"])
assert all(value > 0 for value in r["candidate_durations_ns"])
assert r["copy_verified"] is True
assert r["device_removed_reason"] == 0
' <<<"$transfer"

compound="$($helper --compound-lab --vendor 0x8086 --device 0x56a5 --payload-bytes 4096 --operations 4 --samples 10 --warmups 1 --timeout-ms 5000)"
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["passed"] is True
assert r["backend"] == "wsl-dxcore-d3d12"
assert r["stage"] == "complete"
assert r["mode"] == "paired-compound-transfer-lab"
assert r["primitive"] == "double-buffered-preload"
assert r["vendor_id"] == 0x8086
assert r["device_id"] == 0x56A5
assert r["payload_bytes"] == 4096
assert r["operations_per_sample"] == 4
assert r["samples"] == 10
assert r["warmups"] == 1
assert r["pool_setup_ns"] > 0
assert r["candidate_setup_ns"] > 0
assert len(r["baseline_durations_ns"]) == 10
assert len(r["candidate_durations_ns"]) == 10
assert len(r["correctness_digests"]) == 10
assert all(value > 0 for value in r["baseline_durations_ns"])
assert all(value > 0 for value in r["candidate_durations_ns"])
assert r["copy_verified"] is True
assert r["device_removed_reason"] == 0
' <<<"$compound"

set +e
wrong_device="$($helper --vendor 0x8086 --device 0xffff --timeout-ms 5000)"
wrong_status=$?
invalid_timeout="$($helper --timeout-ms 0)"
timeout_status=$?
invalid_elements="$($helper --elements 0)"
elements_status=$?
oversized_elements="$($helper --elements 1048577)"
oversized_status=$?
set -e

[[ $wrong_status -eq 1 ]]
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["passed"] is False
assert r["stage"] == "select-adapter"
assert r["submitted"] is False
' <<<"$wrong_device"

[[ $timeout_status -eq 2 ]]
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["passed"] is False
assert r["stage"] == "arguments"
assert r["submitted"] is False
' <<<"$invalid_timeout"

[[ $elements_status -eq 2 ]]
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["passed"] is False
assert r["stage"] == "arguments"
assert "elements" in r["detail"]
' <<<"$invalid_elements"

[[ $oversized_status -eq 2 ]]
python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
assert r["passed"] is False
assert r["stage"] == "arguments"
assert "elements" in r["detail"]
' <<<"$oversized_elements"

echo ATOM_ARC_DXCORE_NATIVE_TESTS_READY
