#!/usr/bin/env bash
set -euo pipefail

readonly ARC_GPU_ID="8086:56a5"
readonly ARC_AUDIO_ID="8086:4f92"

if [[ ! -c /dev/kvm || ! -e /dev/vfio/vfio ]]; then
  echo "VFIO verification failed: /dev/kvm or /dev/vfio/vfio is unavailable." >&2
  exit 1
fi
if [[ ! -d /sys/kernel/iommu_groups ]] ||
   ! compgen -G '/sys/kernel/iommu_groups/*' >/dev/null; then
  echo "VFIO verification failed: no active IOMMU groups." >&2
  exit 1
fi

find_devices() {
  local wanted=$1
  local path vendor device
  for path in /sys/bus/pci/devices/*; do
    [[ -r ${path}/vendor && -r ${path}/device ]] || continue
    vendor=$(<"${path}/vendor")
    device=$(<"${path}/device")
    if [[ ${vendor#0x}:${device#0x} == "${wanted}" ]]; then
      basename "${path}"
    fi
  done
}

mapfile -t gpu_bdfs < <(find_devices "${ARC_GPU_ID}")
mapfile -t audio_bdfs < <(find_devices "${ARC_AUDIO_ID}")
if [[ ${#gpu_bdfs[@]} -ne 1 || ${#audio_bdfs[@]} -ne 1 ]]; then
  echo "VFIO verification failed: expected exactly one A380 GPU and audio function." >&2
  printf 'GPU matches: %s\n' "${gpu_bdfs[*]:-none}" >&2
  printf 'Audio matches: %s\n' "${audio_bdfs[*]:-none}" >&2
  exit 1
fi

targets=("${gpu_bdfs[0]}" "${audio_bdfs[0]}")
declare -A target_set=(["${targets[0]}"]=1 ["${targets[1]}"]=1)
declare -A audited_groups=()

echo "Kernel: $(uname -r)"
echo "Command line: $(</proc/cmdline)"
ls -l /dev/kvm /dev/vfio/vfio

for bdf in "${targets[@]}"; do
  device=/sys/bus/pci/devices/${bdf}
  if [[ ! -L ${device}/driver || ! -L ${device}/iommu_group ]]; then
    echo "VFIO verification failed: ${bdf} lacks a driver or IOMMU group." >&2
    exit 1
  fi
  driver=$(basename "$(readlink -f "${device}/driver")")
  group=$(basename "$(readlink -f "${device}/iommu_group")")
  subsystem_vendor=$(<"${device}/subsystem_vendor")
  subsystem_device=$(<"${device}/subsystem_device")
  if [[ ${subsystem_vendor} != 0x172f || ${subsystem_device} != 0x3943 ]]; then
    echo "VFIO verification failed: ${bdf} is not the verified 172f:3943 board." >&2
    exit 1
  fi
  if [[ ${driver} != vfio-pci || ! -e /dev/vfio/${group} ]]; then
    echo "VFIO verification failed: ${bdf} driver=${driver}, group=${group}." >&2
    exit 1
  fi
  lspci -Dnnk -s "${bdf}"

  if [[ -n ${audited_groups[${group}]+x} ]]; then
    continue
  fi
  audited_groups[${group}]=1
  echo "IOMMU group ${group}:"
  for peer in "/sys/kernel/iommu_groups/${group}/devices/"*; do
    peer_bdf=$(basename "${peer}")
    class=$(<"${peer}/class")
    printf '  %s class=%s ' "${peer_bdf}" "${class}"
    lspci -Dnn -s "${peer_bdf}"
    if [[ -z ${target_set[${peer_bdf}]+x} && ${class#0x} != 0604* ]]; then
      echo "VFIO verification failed: unrelated endpoint ${peer_bdf} shares group ${group}." >&2
      exit 1
    fi
  done
done

echo "ATOM_ARC_VFIO_GROUP_AUDIT=PASS"
echo "A380 GPU and audio functions have exclusive vfio-pci ownership of their safe IOMMU groups."
