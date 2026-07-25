#!/usr/bin/env bash
set -euo pipefail

readonly ARC_GPU_ID="8086:56a5"
readonly ARC_AUDIO_ID="8086:4f92"

if [[ ${EUID} -ne 0 ]]; then
  echo "Run with sudo: sudo $0" >&2
  exit 1
fi

if [[ ! -d /sys/kernel/iommu_groups ]] || ! compgen -G '/sys/kernel/iommu_groups/*' >/dev/null; then
  echo "No IOMMU groups exist. Enable VT-d in firmware and run 01-prepare-kvm-host.sh, then reboot." >&2
  exit 1
fi

find_devices() {
  local wanted=$1
  local device vendor id
  for device in /sys/bus/pci/devices/*; do
    [[ -r ${device}/vendor && -r ${device}/device ]] || continue
    vendor=$(<"${device}/vendor")
    id=$(<"${device}/device")
    if [[ ${vendor#0x}:${id#0x} == "${wanted}" ]]; then
      basename "${device}"
    fi
  done
}

mapfile -t gpu_bdfs < <(find_devices "${ARC_GPU_ID}")
mapfile -t audio_bdfs < <(find_devices "${ARC_AUDIO_ID}")
if [[ ${#gpu_bdfs[@]} -ne 1 || ${#audio_bdfs[@]} -ne 1 ]]; then
  echo "Expected one ${ARC_GPU_ID} GPU and one ${ARC_AUDIO_ID} audio function." >&2
  printf 'GPU matches: %s\n' "${gpu_bdfs[*]:-none}" >&2
  printf 'Audio matches: %s\n' "${audio_bdfs[*]:-none}" >&2
  exit 1
fi

targets=("${gpu_bdfs[0]}" "${audio_bdfs[0]}")
declare -A target_set=(["${targets[0]}"]=1 ["${targets[1]}"]=1)

for bdf in "${targets[@]}"; do
  group_link=/sys/bus/pci/devices/${bdf}/iommu_group
  if [[ ! -L ${group_link} ]]; then
    echo "${bdf} has no IOMMU group." >&2
    exit 1
  fi
  group=$(basename "$(readlink -f "${group_link}")")
  echo "${bdf} is in IOMMU group ${group}:"
  for peer in "/sys/kernel/iommu_groups/${group}/devices/"*; do
    peer_bdf=$(basename "${peer}")
    class=$(<"${peer}/class")
    printf '  %s class=%s ' "${peer_bdf}" "${class}"
    lspci -nn -s "${peer_bdf}"

    # PCI bridges may share the group and do not need vfio-pci ownership.
    if [[ -z ${target_set[${peer_bdf}]+x} && ${class#0x} != 0604* ]]; then
      echo "Refusing VFIO binding: unrelated endpoint ${peer_bdf} shares group ${group}." >&2
      exit 1
    fi
  done
done

cat >/etc/modprobe.d/atom-arc-vfio.conf <<EOF
options vfio-pci ids=${ARC_GPU_ID},${ARC_AUDIO_ID} disable_vga=1
softdep i915 pre: vfio-pci
softdep xe pre: vfio-pci
EOF

for module in vfio vfio_iommu_type1 vfio_pci; do
  grep -qxF "${module}" /etc/initramfs-tools/modules || echo "${module}" >>/etc/initramfs-tools/modules
done

cat >/etc/default/grub.d/61-atom-vfio.cfg <<EOF
GRUB_CMDLINE_LINUX_DEFAULT="\$GRUB_CMDLINE_LINUX_DEFAULT vfio-pci.ids=${ARC_GPU_ID},${ARC_AUDIO_ID}"
EOF

update-initramfs -u -k all
update-grub

echo
echo "A380 VFIO early binding is configured after a safe IOMMU-group audit."
echo "Reboot, then run verify-vfio.sh."
