#!/usr/bin/env bash
set -euo pipefail

cycles=1000
while (($#)); do
  case "$1" in
    --cycles)
      cycles=${2:?missing value for --cycles}
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ ! ${cycles} =~ ^[0-9]+$ ]] || ((cycles < 1000)); then
  echo "Phase 0 requires at least 1000 authenticated heartbeat cycles." >&2
  exit 1
fi

# ATOM_ARC_GATE_DISABLED
echo "NATIVE_PHASE0_BLOCKED: full firmware/IRQ/reset/submission testing is outside the active Stage 0 stack." >&2
echo "Pass linux-guest/03-test-native-stage0.sh on the VFIO-owned A380 before implementing or enabling this gate." >&2
exit 1

# shellcheck disable=SC1091
source /etc/os-release
if [[ ${ID:-} != ubuntu ]]; then
  echo "Native Phase 0 testing requires the isolated Ubuntu guest." >&2
  exit 1
fi

mapfile -t arc_bdfs < <(lspci -Dn | awk '$2 ~ /^03/ && tolower($3) == "8086:56a5" {print $1}')
mapfile -t intel_display_bdfs < <(lspci -Dn | awk '$2 ~ /^03/ && tolower($3) ~ /^8086:/ {print $1}')
if [[ ${#arc_bdfs[@]} -ne 1 ]]; then
  echo "NATIVE_PHASE0_BLOCKED: expected exactly one passed-through Arc A380 (8086:56a5); found ${#arc_bdfs[@]}." >&2
  exit 1
fi
if [[ ${#intel_display_bdfs[@]} -ne 1 || ${intel_display_bdfs[0]} != "${arc_bdfs[0]}" ]]; then
  echo "NATIVE_PHASE0_BLOCKED: the guest contains an unexpected Intel display topology." >&2
  exit 1
fi

bdf=${arc_bdfs[0]}
device=/sys/bus/pci/devices/${bdf}
driver=$(basename "$(readlink -f "${device}/driver")")
if [[ ${driver} != atom_arc ]]; then
  echo "NATIVE_PHASE0_BLOCKED: ${bdf} is bound to ${driver}, not atom_arc." >&2
  exit 1
fi

state_root=${device}/atom_arc
required_attributes=(
  architecture driver_state firmware_authenticated submission_enabled
  bar0_bytes guc_in_reset boot_hash_valid wopcm_locked
)
for attribute in "${required_attributes[@]}"; do
  if [[ ! -r ${state_root}/${attribute} ]]; then
    echo "NATIVE_PHASE0_BLOCKED: missing ${state_root}/${attribute}." >&2
    exit 1
  fi
done

[[ $(<"${state_root}/architecture") == standalone-linux-pci ]] || {
  echo "NATIVE_PHASE0_BLOCKED: wrong architecture marker." >&2
  exit 1
}
[[ $(<"${state_root}/firmware_authenticated") == 1 ]] || {
  echo "NATIVE_PHASE0_BLOCKED: production firmware is not hardware-authenticated." >&2
  exit 1
}
[[ $(<"${state_root}/submission_enabled") == 1 ]] || {
  echo "NATIVE_PHASE0_BLOCKED: native command submission is locked." >&2
  exit 1
}
[[ $(<"${state_root}/driver_state") == submission-ready ]] || {
  echo "NATIVE_PHASE0_BLOCKED: driver_state is not submission-ready." >&2
  exit 1
}
[[ $(<"${state_root}/guc_in_reset") == 0 ]] || {
  echo "NATIVE_PHASE0_BLOCKED: GuC remains in reset." >&2
  exit 1
}
[[ $(<"${state_root}/boot_hash_valid") == 1 ]] || {
  echo "NATIVE_PHASE0_BLOCKED: GuC boot hash is not valid." >&2
  exit 1
}
[[ $(<"${state_root}/wopcm_locked") == 1 ]] || {
  echo "NATIVE_PHASE0_BLOCKED: WOPCM is not locked." >&2
  exit 1
}
[[ $(<"${state_root}/bar0_bytes") =~ ^[1-9][0-9]*$ && -r ${device}/resource0 ]] || {
  echo "NATIVE_PHASE0_BLOCKED: BAR0 MMIO ownership is not established." >&2
  exit 1
}

command_word=$(setpci -s "${bdf}" COMMAND)
if ((((16#${command_word}) & 0x4) == 0)); then
  echo "NATIVE_PHASE0_BLOCKED: PCI bus mastering is disabled." >&2
  exit 1
fi

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
binary=${ATOM_ARCCTL_BIN:-${project_root}/target/release/atom-arcctl}
if [[ ! -x ${binary} ]]; then
  echo "NATIVE_PHASE0_BLOCKED: native heartbeat executable is missing at ${binary}." >&2
  exit 1
fi

interrupt_total() {
  awk -v bdf="${bdf}" '
    index($0, bdf) || index($0, "atom_arc") {
      for (i = 2; i <= NF; i++) {
        if ($i ~ /^[0-9]+$/) total += $i
      }
    }
    END { print total + 0 }
  ' /proc/interrupts
}

run_id="atom-native-phase0-$(date -u +%Y%m%dT%H%M%SZ)-$$"
raw_result=$(mktemp)
trap 'rm -f "${raw_result}"' EXIT
interrupts_before=$(interrupt_total)

"${binary}" native-heartbeat \
  --cycles "${cycles}" \
  --exercise-reset-recovery \
  --output "${raw_result}"

interrupts_after=$(interrupt_total)
if ((interrupts_after <= interrupts_before)); then
  echo "NATIVE_PHASE0_BLOCKED: no Atom interrupt advanced during heartbeat execution." >&2
  exit 1
fi

evidence_relative=experiments/results/atom-arc-native-phase0-latest.json
evidence_path=${project_root}/${evidence_relative}
proof_path=${project_root}/atom-arc-native-live-proof.json
mkdir -p "$(dirname -- "${evidence_path}")"

python3 - \
  "${raw_result}" "${evidence_path}" "${proof_path}" "${run_id}" \
  "${bdf}" "${cycles}" "${interrupts_before}" "${interrupts_after}" <<'PY'
import datetime
import json
import pathlib
import sys

raw_path, evidence_path, proof_path, run_id, bdf, cycles, before, after = sys.argv[1:]
with open(raw_path, encoding="utf-8") as stream:
    raw = json.load(stream)

required = {
    "schema": 1,
    "backend": "atom-native",
    "driver": "atom_arc",
    "correctness_passed": True,
    "reset_recovery_verified": True,
    "reset_count": 0,
    "timeout_count": 0,
    "mismatch_count": 0,
    "device_error_count": 0,
}
for key, expected in required.items():
    if raw.get(key) != expected:
        raise SystemExit(f"NATIVE_PHASE0_BLOCKED: raw result {key} must equal {expected!r}")

cycle_count = raw.get("heartbeat_cycles")
heartbeat_rate = raw.get("heartbeat_per_second")
latency_ns = raw.get("latency_ns")
if not isinstance(cycle_count, int) or cycle_count < int(cycles):
    raise SystemExit("NATIVE_PHASE0_BLOCKED: insufficient heartbeat cycles")
for label, value in (("heartbeat_per_second", heartbeat_rate), ("latency_ns", latency_ns)):
    if not isinstance(value, (int, float)) or isinstance(value, bool) or value <= 0:
        raise SystemExit(f"NATIVE_PHASE0_BLOCKED: {label} must be positive")

tested_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
integration_test = "linux-guest/04-test-native-phase0.sh"
measurement = {
    "sample_count": raw.get("sample_count", cycle_count),
    "heartbeat_cycles": cycle_count,
    "heartbeat_per_second": heartbeat_rate,
    "latency_ns": latency_ns,
    "reset_count": 0,
    "timeout_count": 0,
    "mismatch_count": 0,
    "device_error_count": 0,
}
evidence = {
    "schema": 1,
    "result": "passed",
    "native_path": True,
    "integration_test": integration_test,
    "run_id": run_id,
    "phase": 0,
    "driver": "atom_arc",
    "device": {"pci_bdf": bdf, "pci_id": "8086:56a5", "bound_driver": "atom_arc"},
    "measurement": measurement,
    "interrupts_before": int(before),
    "interrupts_after": int(after),
    "tested_at_utc": tested_at,
}
proof = {
    "schema": 1,
    "phase": 0,
    "path_kind": "atom-native",
    "driver": "atom_arc",
    "device": evidence["device"],
    "firmware": {"authenticated": True},
    "runtime": {
        "pci_bus_master": True,
        "bar0_mmio": True,
        "submission_enabled": True,
        "interrupts_observed": True,
        "reset_recovery_verified": True,
    },
    "correctness": {"passed": True},
    "measurement": measurement,
    "tested_at_utc": tested_at,
    "integration_test": integration_test,
    "evidence_file": "experiments/results/atom-arc-native-phase0-latest.json",
}
for path, payload in ((evidence_path, evidence), (proof_path, proof)):
    destination = pathlib.Path(path)
    temporary = destination.with_suffix(destination.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    temporary.replace(destination)
PY

echo "ATOM_ARC_NATIVE_PROOF=atom-arc-native-live-proof.json"
echo "ATOM_ARC_NATIVE_EVIDENCE=${evidence_relative}"
echo "ATOM_ARC_NATIVE_RUN_ID=${run_id}"
