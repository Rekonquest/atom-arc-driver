#!/usr/bin/env bash
set -euo pipefail

mapfile -t arc_bdfs < <(
  lspci -Dn | awk '$2 ~ /^03/ && tolower($3) == "8086:56a5" {print $1}'
)
if [[ ${#arc_bdfs[@]} -ne 1 ]]; then
  echo "STAGE0_BLOCKED: expected exactly one Intel Arc A380; found ${#arc_bdfs[@]}." >&2
  exit 1
fi

bdf=${arc_bdfs[0]}
device=/sys/bus/pci/devices/${bdf}
driver=$(basename "$(readlink -f "${device}/driver")")
if [[ ${driver} != atom_arc ]]; then
  echo "STAGE0_BLOCKED: ${bdf} is bound to ${driver}, not atom_arc." >&2
  exit 1
fi

state_root=${device}/atom_arc
required_attributes=(
  architecture stage driver_state pci_bdf pci_identity subsystem_identity
  revision class_code bar0_start bar0_bytes bar2_start bar2_bytes
  dma_address_bits regions_claimed mmio_mapped mmio_read_count
  bus_master_enabled firmware_stage
)
for attribute in "${required_attributes[@]}"; do
  if [[ ! -r ${state_root}/${attribute} ]]; then
    echo "STAGE0_BLOCKED: missing ${state_root}/${attribute}." >&2
    exit 1
  fi
done

declare -A expected=(
  [architecture]=standalone-linux-pci
  [stage]=0
  [driver_state]=probe-only-locked
  [pci_bdf]="${bdf}"
  [pci_identity]=8086:56a5
  [subsystem_identity]=172f:3943
  [revision]=0x05
  [class_code]=0x030000
  [dma_address_bits]=46
  [regions_claimed]=1
  [mmio_mapped]=1
  [mmio_read_count]=0
  [bus_master_enabled]=0
  [firmware_stage]=not-started
)
for attribute in "${!expected[@]}"; do
  actual=$(<"${state_root}/${attribute}")
  if [[ ${actual} != "${expected[${attribute}]}" ]]; then
    echo "STAGE0_BLOCKED: ${attribute}=${actual}, expected ${expected[${attribute}]}." >&2
    exit 1
  fi
done

for attribute in bar0_bytes bar2_bytes; do
  value=$(<"${state_root}/${attribute}")
  if [[ ! ${value} =~ ^[1-9][0-9]*$ ]]; then
    echo "STAGE0_BLOCKED: ${attribute} is not a positive byte count." >&2
    exit 1
  fi
done
for attribute in bar0_start bar2_start; do
  value=$(<"${state_root}/${attribute}")
  if [[ ! ${value} =~ ^0x[1-9a-fA-F][0-9a-fA-F]*$ ]]; then
    echo "STAGE0_BLOCKED: ${attribute} is not a nonzero address." >&2
    exit 1
  fi
done

for forbidden in firmware_authenticated submission_enabled guc_status_raw \
  boot_hash_raw wopcm_size_raw wopcm_offset_raw; do
  if [[ -e ${state_root}/${forbidden} ]]; then
    echo "STAGE0_BLOCKED: premature Stage 1+ attribute exists: ${forbidden}." >&2
    exit 1
  fi
done

for attribute in "${required_attributes[@]}"; do
  printf '%s=%s\n' "${attribute}" "$(<"${state_root}/${attribute}")"
done
echo "ATOM_ARC_NATIVE_STAGE0_PROBE=PASS"
