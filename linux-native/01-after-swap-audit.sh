#!/usr/bin/env bash
set -euo pipefail

mapfile -t arc_bdfs < <(
  lspci -Dn | awk '$2 ~ /^03/ && tolower($3) == "8086:56a5" {print $1}'
)
mapfile -t audio_bdfs < <(
  lspci -Dn | awk '$2 ~ /^04/ && tolower($3) == "8086:4f92" {print $1}'
)

if [[ ${#arc_bdfs[@]} -ne 1 ]]; then
  echo "POST_SWAP_BLOCKED: expected one A380 (8086:56a5); found ${#arc_bdfs[@]}." >&2
  exit 1
fi
if [[ ${#audio_bdfs[@]} -ne 1 ]]; then
  echo "POST_SWAP_BLOCKED: expected one A380 audio function (8086:4f92); found ${#audio_bdfs[@]}." >&2
  exit 1
fi

bdf=${arc_bdfs[0]}
device=/sys/bus/pci/devices/${bdf}
if [[ ! -L ${device}/driver ]]; then
  echo "POST_SWAP_BLOCKED: ${bdf} has no kernel driver." >&2
  exit 1
fi
driver=$(basename "$(readlink -f "${device}/driver")")
if [[ ${driver} != xe && ${driver} != i915 ]]; then
  echo "POST_SWAP_BLOCKED: ${bdf} is bound to ${driver}, expected xe or i915." >&2
  exit 1
fi

echo '=== ARC PCI OWNERSHIP ==='
lspci -nnk -s "${bdf}"
lspci -nnk -s "${audio_bdfs[0]}"
printf 'gpu_bdf=%s\naudio_bdf=%s\ndriver=%s\nboot_vga=%s\n' \
  "${bdf}" "${audio_bdfs[0]}" "${driver}" "$(<"${device}/boot_vga")"

echo '=== ARC DRM NODES ==='
find "${device}/drm" -maxdepth 1 -mindepth 1 -printf '%f\n' 2>/dev/null | sort || true
if ! find "${device}/drm" -maxdepth 1 -name 'renderD*' -print -quit 2>/dev/null | grep -q .; then
  echo 'POST_SWAP_BLOCKED: the A380 has no DRM render node.' >&2
  exit 1
fi

echo '=== KERNEL LOG ==='
if [[ ${EUID} -eq 0 ]]; then
  dmesg | grep -Ei "(${bdf}|xe |i915 |GuC|HuC|GSC|drm)" | tail -n 160 || true
else
  journalctl -k -b --no-pager 2>/dev/null \
    | grep -Ei "(${bdf}|xe |i915 |GuC|HuC|GSC|drm)" | tail -n 160 || true
fi

echo '=== INTEL DEVICE LIST ==='
intel_gpu_top -L

echo '=== VULKAN ==='
vulkaninfo --summary | grep -E 'GPU[0-9]+|deviceName|driverName|driverInfo' || true
if ! vulkaninfo --summary 2>/dev/null | grep -Eiq 'Arc.*A380|A380.*Arc'; then
  echo 'POST_SWAP_BLOCKED: Vulkan does not report the Arc A380.' >&2
  exit 1
fi

echo '=== OPENCL ==='
clinfo -l
if ! clinfo 2>/dev/null | grep -Eiq 'Arc.*A380|A380.*Arc'; then
  echo 'POST_SWAP_BLOCKED: OpenCL does not report the Arc A380.' >&2
  exit 1
fi

echo '=== MODULE AND FIRMWARE ==='
modinfo -F filename "${driver}"
for firmware in \
  /lib/firmware/i915/dg2_guc_70.bin \
  /lib/firmware/i915/dg2_huc_gsc.bin \
  /lib/firmware/i915/dg2_dmc_ver2_08.bin; do
  test -s "${firmware}"
  sha256sum "${firmware}"
done

echo ATOM_ARC_POST_SWAP_BASELINE_PASS
