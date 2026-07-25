#!/usr/bin/env bash
set -euo pipefail

readonly ARC_GPU_ID="8086:56a5"
readonly ARC_AUDIO_ID="8086:4f92"
readonly VM_NAME="atom-arc-ubuntu"

usage() {
  echo "Usage: sudo $0 /absolute/path/to/atom-arc-ubuntu.qcow2" >&2
}

if [[ ${EUID} -ne 0 || $# -ne 1 ]]; then
  usage
  exit 1
fi

disk=$(readlink -f "$1")
if [[ ! -f ${disk} ]]; then
  echo "Guest disk does not exist: ${disk}" >&2
  exit 1
fi

script_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
"${script_root}/verify-vfio.sh"

find_bdf() {
  local id=$1
  lspci -Dnn | awk -v id="${id}" 'BEGIN{IGNORECASE=1} index($0,id){print $1}'
}

gpu_bdf=$(find_bdf "${ARC_GPU_ID}")
audio_bdf=$(find_bdf "${ARC_AUDIO_ID}")
if [[ $(wc -w <<<"${gpu_bdf}") -ne 1 || $(wc -w <<<"${audio_bdf}") -ne 1 ]]; then
  echo "Could not uniquely identify the A380 GPU and audio functions." >&2
  exit 1
fi

for bdf in "${gpu_bdf}" "${audio_bdf}"; do
  driver=$(basename "$(readlink -f "/sys/bus/pci/devices/${bdf}/driver")")
  if [[ ${driver} != vfio-pci ]]; then
    echo "${bdf} is bound to ${driver}, not vfio-pci." >&2
    exit 1
  fi
done

if virsh dominfo "${VM_NAME}" >/dev/null 2>&1; then
  echo "VM ${VM_NAME} already exists; refusing to overwrite it." >&2
  exit 1
fi

virt-install \
  --name "${VM_NAME}" \
  --import \
  --machine q35 \
  --boot uefi \
  --memory 16384 \
  --vcpus 8 \
  --cpu host-passthrough \
  --os-variant ubuntu24.04 \
  --disk "path=${disk},format=qcow2,bus=virtio,cache=none" \
  --network network=default,model=virtio \
  --hostdev "${gpu_bdf}" \
  --hostdev "${audio_bdf}" \
  --graphics spice,listen=127.0.0.1 \
  --video virtio \
  --console pty,target_type=serial \
  --noautoconsole

virsh autostart "${VM_NAME}" --disable
virsh dominfo "${VM_NAME}"
echo "Atom Arc Ubuntu guest started with the physical A380 attached and a virtual recovery console retained."
