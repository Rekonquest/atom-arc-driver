#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
  echo 'Run with sudo.' >&2
  exit 1
fi

kernel_release="$(uname -r)"
project_root=/home/jesse/Projects/atom-arc-driver
module="/opt/atom-arc/${kernel_release}/atom_arc.ko"
verify_script="${project_root}/linux-guest/02-verify-atom-driver.sh"

mapfile -t arc_bdfs < <(
  lspci -Dn | awk '$2 ~ /^03/ && tolower($3) == "8086:56a5" {print $1}'
)
mapfile -t audio_bdfs < <(
  lspci -Dn | awk '$2 ~ /^04/ && tolower($3) == "8086:4f92" {print $1}'
)
mapfile -t igpu_bdfs < <(
  lspci -Dn | awk '$2 ~ /^03/ && tolower($3) == "8086:4680" {print $1}'
)

if [[ ${#arc_bdfs[@]} -ne 1 || ${#audio_bdfs[@]} -ne 1 ]]; then
  echo 'HOST_STAGE0_BLOCKED: exact A380 GPU/audio topology is absent.' >&2
  exit 1
fi
if [[ ${#igpu_bdfs[@]} -ne 1 ]]; then
  echo 'HOST_STAGE0_BLOCKED: the Alder Lake recovery iGPU is absent.' >&2
  exit 1
fi

bdf=${arc_bdfs[0]}
device=/sys/bus/pci/devices/${bdf}
igpu=/sys/bus/pci/devices/${igpu_bdfs[0]}

if [[ $(<"${device}/boot_vga") != 0 ]]; then
  echo 'HOST_STAGE0_BLOCKED: the A380 is boot VGA; use the motherboard iGPU first.' >&2
  exit 1
fi
if [[ ! -L ${igpu}/driver ]]; then
  echo 'HOST_STAGE0_BLOCKED: the recovery iGPU has no bound driver.' >&2
  exit 1
fi

connected=0
while IFS= read -r status_file; do
  if [[ $(<"${status_file}") == connected ]]; then
    connected=1
  fi
done < <(find "${igpu}/drm" -path '*/card*-*/status' -type f 2>/dev/null)
if [[ ${connected} -ne 1 ]]; then
  echo 'HOST_STAGE0_BLOCKED: no connected display is owned by the recovery iGPU.' >&2
  exit 1
fi

if [[ ! -L ${device}/driver ]]; then
  echo 'HOST_STAGE0_BLOCKED: the A380 has no original driver to restore.' >&2
  exit 1
fi
original_driver=$(basename "$(readlink -f "${device}/driver")")
if [[ ${original_driver} != xe && ${original_driver} != i915 ]]; then
  echo "HOST_STAGE0_BLOCKED: unexpected original driver ${original_driver}." >&2
  exit 1
fi

mapfile -t drm_nodes < <(
  find "${device}/drm" -maxdepth 1 -mindepth 1 -printf '%f\n' 2>/dev/null \
    | grep -E '^(card|renderD|controlD)[0-9]+$' || true
)
for node in "${drm_nodes[@]}"; do
  if [[ -e /dev/dri/${node} ]] && fuser "/dev/dri/${node}" >/dev/null 2>&1; then
    echo "HOST_STAGE0_BLOCKED: /dev/dri/${node} is open by another process." >&2
    fuser -v "/dev/dri/${node}" >&2 || true
    exit 1
  fi
done

test -s "${module}"
test -x "${verify_script}"
if [[ $(modinfo -F vermagic "${module}" | awk '{print $1}') != "${kernel_release}" ]]; then
  echo 'HOST_STAGE0_BLOCKED: staged module does not match the running kernel.' >&2
  exit 1
fi
if find "/lib/modules/${kernel_release}" -type f -name 'atom_arc.ko*' -print -quit | grep -q .; then
  echo 'HOST_STAGE0_BLOCKED: atom_arc must remain outside automatic module discovery.' >&2
  exit 1
fi

restored=0
restore_original() {
  status=$?
  set +e
  if [[ -L ${device}/driver ]] &&
      [[ $(basename "$(readlink -f "${device}/driver")") == atom_arc ]]; then
    echo "${bdf}" > /sys/bus/pci/drivers/atom_arc/unbind
  fi
  rmmod atom_arc 2>/dev/null || true
  modprobe "${original_driver}"
  printf '%s\n' "${original_driver}" > "${device}/driver_override"
  echo "${bdf}" > /sys/bus/pci/drivers_probe
  printf '\n' > "${device}/driver_override"
  if [[ -L ${device}/driver ]] &&
      [[ $(basename "$(readlink -f "${device}/driver")") == "${original_driver}" ]]; then
    restored=1
  fi
  if [[ ${restored} -ne 1 ]]; then
    echo "HOST_STAGE0_RECOVERY_REQUIRED: could not restore ${original_driver} to ${bdf}." >&2
    exit 1
  fi
  exit "${status}"
}
trap restore_original EXIT

echo "${bdf}" > "/sys/bus/pci/drivers/${original_driver}/unbind"
printf '%s\n' atom_arc > "${device}/driver_override"

for cycle in 1 2; do
  insmod "${module}"
  echo "${bdf}" > /sys/bus/pci/drivers_probe
  bash "${verify_script}"

  command_word=$(setpci -s "${bdf}" COMMAND)
  if ((((16#${command_word}) & 0x4) != 0)); then
    echo "HOST_STAGE0_BLOCKED: bus mastering enabled in cycle ${cycle}." >&2
    exit 1
  fi

  echo "${bdf}" > /sys/bus/pci/drivers/atom_arc/unbind
  rmmod atom_arc
  if [[ -L ${device}/driver || -d /sys/module/atom_arc ]]; then
    echo "HOST_STAGE0_BLOCKED: clean unload failed in cycle ${cycle}." >&2
    exit 1
  fi
done

printf '%s\n' "${original_driver}" > "${device}/driver_override"
modprobe "${original_driver}"
echo "${bdf}" > /sys/bus/pci/drivers_probe
printf '\n' > "${device}/driver_override"
if [[ ! -L ${device}/driver ]] ||
    [[ $(basename "$(readlink -f "${device}/driver")") != "${original_driver}" ]]; then
  echo "HOST_STAGE0_RECOVERY_REQUIRED: final ${original_driver} restore failed." >&2
  exit 1
fi
restored=1
trap - EXIT

echo 'ATOM_ARC_NATIVE_HOST_STAGE0_CYCLES=2'
echo "ATOM_ARC_NATIVE_HOST_RESTORED_DRIVER=${original_driver}"
echo ATOM_ARC_NATIVE_HOST_STAGE0_SMOKE_PASS
