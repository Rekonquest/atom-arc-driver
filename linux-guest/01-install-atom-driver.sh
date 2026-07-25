#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
  echo "Run with sudo: sudo $0" >&2
  exit 1
fi

# shellcheck disable=SC1091
source /etc/os-release
if [[ ${ID:-} != ubuntu ]]; then
  echo "This installer targets the isolated Atom Ubuntu guest; detected ${ID:-unknown}." >&2
  exit 1
fi

mapfile -t arc_bdfs < <(lspci -Dn | awk '$2 ~ /^03/ && tolower($3) == "8086:56a5" {print $1}')
mapfile -t intel_display_bdfs < <(lspci -Dn | awk '$2 ~ /^03/ && tolower($3) ~ /^8086:/ {print $1}')
if [[ ${#arc_bdfs[@]} -ne 1 ]]; then
  echo "Expected exactly one passed-through Intel Arc A380 (8086:56a5)." >&2
  exit 1
fi
if [[ ${#intel_display_bdfs[@]} -ne 1 || ${intel_display_bdfs[0]} != "${arc_bdfs[0]}" ]]; then
  echo "Refusing installation: another Intel display device exists in this guest." >&2
  printf 'Intel display functions: %s\n' "${intel_display_bdfs[*]:-none}" >&2
  exit 1
fi

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
module_dir=${project_root}/kernel/atom_arc
kernel_release=$(uname -r)
kernel_build=/lib/modules/${kernel_release}/build
required_packages=(
  "linux-headers-${kernel_release}"
  build-essential
  kmod
  pciutils
  python3
  sparse
)
missing_packages=()
for package in "${required_packages[@]}"; do
  if ! dpkg-query -W -f='${db:Status-Abbrev}' "${package}" 2>/dev/null | grep -q '^ii '; then
    missing_packages+=("${package}")
  fi
done
if [[ ${#missing_packages[@]} -gt 0 ]]; then
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y "${missing_packages[@]}"
fi
if [[ ! -d ${kernel_build} ]]; then
  echo "Kernel build tree is still missing after package installation: ${kernel_build}" >&2
  exit 1
fi

make -C "${module_dir}" clean
make -C "${module_dir}"
make -C "${module_dir}" sparse
install -D -m 0644 "${module_dir}/atom_arc.ko" \
  "/lib/modules/${kernel_release}/updates/atom_arc.ko"
depmod -a "${kernel_release}"

cat >/etc/modprobe.d/atom-arc-guest.conf <<'EOF'
# The Atom guest receives only the A380. Prevent Intel's drivers from claiming it.
blacklist i915
blacklist xe
EOF
grep -qxF atom_arc /etc/initramfs-tools/modules || echo atom_arc >>/etc/initramfs-tools/modules
update-initramfs -u -k "${kernel_release}"

echo
echo "Installed independent Atom Arc module for ${kernel_release}."
echo "The existing GPU driver was not unbound live. Reboot this guest, then run 02-verify-atom-driver.sh."
