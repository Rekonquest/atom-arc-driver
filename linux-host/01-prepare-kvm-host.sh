#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
  echo "Run with sudo: sudo $0" >&2
  exit 1
fi

# shellcheck disable=SC1091
source /etc/os-release
if [[ ${ID:-} != ubuntu ]]; then
  echo "This setup targets a native Ubuntu host; detected ${ID:-unknown}." >&2
  exit 1
fi

operator=${SUDO_USER:-}
if [[ -z ${operator} || ${operator} == root ]]; then
  echo "Run through sudo from the account that will manage the VM." >&2
  exit 1
fi

apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y \
  bridge-utils \
  cpu-checker \
  libvirt-clients \
  libvirt-daemon-system \
  ovmf \
  pciutils \
  qemu-kvm \
  virt-manager \
  virtinst

adduser "${operator}" libvirt
adduser "${operator}" kvm

install -d -m 0755 /etc/default/grub.d
cat >/etc/default/grub.d/60-atom-iommu.cfg <<'EOF'
GRUB_CMDLINE_LINUX_DEFAULT="$GRUB_CMDLINE_LINUX_DEFAULT intel_iommu=on iommu=pt"
EOF

update-grub
systemctl enable --now libvirtd
kvm-ok

echo
echo "KVM packages and Intel IOMMU boot flags are installed."
echo "Reboot the native Ubuntu host, then run 02-bind-arc-vfio.sh."
