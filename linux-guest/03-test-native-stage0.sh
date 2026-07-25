#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
  echo "Run with sudo: sudo bash $0" >&2
  exit 1
fi

# shellcheck disable=SC1091
source /etc/os-release
if [[ ${ID:-} != ubuntu ]]; then
  echo "STAGE0_BLOCKED: native Stage 0 requires the isolated Ubuntu guest." >&2
  exit 1
fi

mapfile -t arc_bdfs < <(
  lspci -Dn | awk '$2 ~ /^03/ && tolower($3) == "8086:56a5" {print $1}'
)
mapfile -t arc_audio_bdfs < <(
  lspci -Dn | awk '$2 ~ /^04/ && tolower($3) == "8086:4f92" {print $1}'
)
mapfile -t intel_display_bdfs < <(
  lspci -Dn | awk '$2 ~ /^03/ && tolower($3) ~ /^8086:/ {print $1}'
)
if [[ ${#arc_bdfs[@]} -ne 1 ]]; then
  echo "STAGE0_BLOCKED: expected one passed-through A380 (8086:56a5); found ${#arc_bdfs[@]}." >&2
  exit 1
fi
if [[ ${#arc_audio_bdfs[@]} -ne 1 ]]; then
  echo "STAGE0_BLOCKED: expected one passed-through A380 audio function (8086:4f92); found ${#arc_audio_bdfs[@]}." >&2
  exit 1
fi
if [[ ${#intel_display_bdfs[@]} -ne 1 ||
      ${intel_display_bdfs[0]} != "${arc_bdfs[0]}" ]]; then
  echo "STAGE0_BLOCKED: the guest has an unexpected Intel display topology." >&2
  exit 1
fi

bdf=${arc_bdfs[0]}
device=/sys/bus/pci/devices/${bdf}
project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
module_dir=${project_root}/kernel/atom_arc
module=${module_dir}/atom_arc.ko
installed_module=/lib/modules/$(uname -r)/updates/atom_arc.ko
verify_script=${project_root}/linux-guest/02-verify-atom-driver.sh
evidence=${project_root}/experiments/results/atom-arc-native-stage0-latest.json

cleanup() {
  set +e
  if [[ -L ${device}/driver &&
        $(basename "$(readlink -f "${device}/driver")") == atom_arc ]]; then
    echo "${bdf}" > /sys/bus/pci/drivers/atom_arc/unbind
  fi
  modprobe -r atom_arc
  printf '\n' > "${device}/driver_override"
}
trap cleanup EXIT

make -C "${module_dir}" clean
make -C "${module_dir}"
make -C "${module_dir}" sparse
module_sha256=$(sha256sum "${module}" | awk '{print $1}')
bash "${module_dir}/check-stage0-policy.sh" "${module}"
main_source_sha256=$(sha256sum "${module_dir}/atom_arc_main.c" | awk '{print $1}')
makefile_sha256=$(sha256sum "${module_dir}/Makefile" | awk '{print $1}')
policy_sha256=$(sha256sum "${module_dir}/check-stage0-policy.sh" | awk '{print $1}')

if [[ -L ${device}/driver ]]; then
  current_driver=$(basename "$(readlink -f "${device}/driver")")
  echo "${bdf}" > "/sys/bus/pci/drivers/${current_driver}/unbind"
fi
modprobe -r atom_arc 2>/dev/null || true
install -D -m 0644 "${module}" "${installed_module}"
depmod -a "$(uname -r)"
installed_sha256=$(sha256sum "${installed_module}" | awk '{print $1}')
if [[ ${installed_sha256} != "${module_sha256}" ]]; then
  echo "STAGE0_BLOCKED: installed module hash differs from the tested build." >&2
  exit 1
fi

printf '%s\n' atom_arc > "${device}/driver_override"

probe_cycles=0
clean_unloads=0
for cycle in 1 2; do
  insmod "${installed_module}"
  if [[ ! -L ${device}/driver ||
        $(basename "$(readlink -f "${device}/driver")") != atom_arc ]]; then
    echo "${bdf}" > /sys/bus/pci/drivers_probe
  fi
  bash "${verify_script}"

  command_word=$(setpci -s "${bdf}" COMMAND)
  if ((((16#${command_word}) & 0x4) != 0)); then
    echo "STAGE0_BLOCKED: PCI bus mastering is enabled in probe cycle ${cycle}." >&2
    exit 1
  fi

  state_root=${device}/atom_arc
  if [[ ${cycle} -eq 2 ]]; then
    pci_identity=$(<"${state_root}/pci_identity")
    subsystem_identity=$(<"${state_root}/subsystem_identity")
    revision=$(<"${state_root}/revision")
    class_code=$(<"${state_root}/class_code")
    bar0_start=$(<"${state_root}/bar0_start")
    bar0_bytes=$(<"${state_root}/bar0_bytes")
    bar2_start=$(<"${state_root}/bar2_start")
    bar2_bytes=$(<"${state_root}/bar2_bytes")
  fi

  echo "${bdf}" > /sys/bus/pci/drivers/atom_arc/unbind
  rmmod atom_arc
  if [[ -L ${device}/driver || -d /sys/module/atom_arc ]]; then
    echo "STAGE0_BLOCKED: clean unload failed in probe cycle ${cycle}." >&2
    exit 1
  fi
  ((probe_cycles += 1))
  ((clean_unloads += 1))
done

printf '\n' > "${device}/driver_override"
trap - EXIT

mkdir -p "$(dirname -- "${evidence}")"
run_id="atom-native-stage0-$(date -u +%Y%m%dT%H%M%SZ)-$$"
python3 - "${evidence}" "${run_id}" "${bdf}" "${arc_audio_bdfs[0]}" \
  "${module_sha256}" "${main_source_sha256}" "${makefile_sha256}" \
  "${policy_sha256}" "$(uname -r)" "${probe_cycles}" "${clean_unloads}" \
  "${pci_identity}" "${subsystem_identity}" "${revision}" "${class_code}" \
  "${bar0_start}" "${bar0_bytes}" "${bar2_start}" "${bar2_bytes}" <<'PY'
import datetime
import json
import pathlib
import sys

(
    evidence_path,
    run_id,
    gpu_bdf,
    audio_bdf,
    module_sha256,
    main_source_sha256,
    makefile_sha256,
    policy_sha256,
    kernel_release,
    probe_cycles,
    clean_unloads,
    pci_identity,
    subsystem_identity,
    revision,
    class_code,
    bar0_start,
    bar0_bytes,
    bar2_start,
    bar2_bytes,
) = sys.argv[1:]
payload = {
    "schema": 1,
    "result": "passed",
    "native_path": True,
    "stage": 0,
    "run_id": run_id,
    "driver": "atom_arc",
    "device": {
        "gpu_bdf": gpu_bdf,
        "audio_bdf": audio_bdf,
        "pci_id": pci_identity,
        "audio_pci_id": "8086:4f92",
        "subsystem_id": subsystem_identity,
        "revision": revision,
        "class_code": class_code,
        "bound_driver_during_probe": "atom_arc",
    },
    "ownership": {
        "probe_cycles": int(probe_cycles),
        "clean_unloads": int(clean_unloads),
        "bar0_start": bar0_start,
        "bar0_bytes": int(bar0_bytes),
        "bar2_start": bar2_start,
        "bar2_bytes": int(bar2_bytes),
        "dma_address_bits": 46,
        "regions_claimed": True,
        "mmio_mapped": True,
        "mmio_read_count": 0,
        "bus_master_enabled": False,
    },
    "stage1_activity_absence": {
        "basis": "source deny policy, module symbol deny policy, and Stage 0 sysfs contract",
        "firmware_started": False,
        "irq_allocated": False,
        "dma_transfer_attempted": False,
        "reset_attempted": False,
        "submission_attempted": False,
    },
    "build": {
        "kernel_release": kernel_release,
        "module_sha256": module_sha256,
        "source_policy_passed": True,
        "source_hashes": {
            "atom_arc_main.c": main_source_sha256,
            "Makefile": makefile_sha256,
            "check-stage0-policy.sh": policy_sha256,
        },
    },
    "tested_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
}
path = pathlib.Path(evidence_path)
temporary = path.with_suffix(path.suffix + ".tmp")
temporary.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
temporary.replace(path)
PY

echo "ATOM_ARC_NATIVE_STAGE0_EVIDENCE=experiments/results/atom-arc-native-stage0-latest.json"
echo "ATOM_ARC_NATIVE_STAGE0_RUN_ID=${run_id}"
