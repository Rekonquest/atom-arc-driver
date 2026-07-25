#!/usr/bin/env bash
set -euo pipefail

script_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source_file=${script_root}/atom_arc_main.c
makefile=${script_root}/Makefile
module=${1:-}

if [[ ! -r ${source_file} || ! -r ${makefile} ]]; then
  echo "STAGE0_POLICY_BLOCKED: active source or Makefile is unreadable." >&2
  exit 1
fi
for tool in cut grep; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "STAGE0_POLICY_BLOCKED: required policy tool is missing: ${tool}." >&2
    exit 1
  fi
done

if [[ $(grep -Ec '^atom_arc-y[[:space:]]*[:+?]?=' "${makefile}") -ne 1 ]] ||
   ! grep -Eq '^atom_arc-y[[:space:]]*:=[[:space:]]*atom_arc_main\.o[[:space:]]*$' "${makefile}"; then
  echo "STAGE0_POLICY_BLOCKED: atom_arc.ko must link only atom_arc_main.o." >&2
  exit 1
fi

forbidden_source_patterns=(
  '(module_param|MODULE_FIRMWARE)[[:space:]]*\('
  '(request_firmware|request_firmware_direct|release_firmware)[[:space:]]*\('
  '(readb|readw|readl|readq|writeb|writew|writel|writeq|ioread8|ioread16|ioread32|ioread64|iowrite8|iowrite16|iowrite32|iowrite64|memcpy_fromio|memcpy_toio)[[:space:]]*\('
  '(dma_alloc[^[:space:](]*|dma_map[^[:space:](]*|dmaengine_[^[:space:](]*)[[:space:]]*\('
  '(pci_alloc_irq_vectors|request_irq|request_threaded_irq|devm_request_irq)[[:space:]]*\('
  '(pci_reset_function|pci_try_reset_function|pci_reset_bus|reset_control_[a-z_]+)[[:space:]]*\('
  'pci_set_master[[:space:]]*\('
  '(drm_dev_register|misc_register|cdev_add|register_chrdev)[[:space:]]*\('
  '\.(unlocked_ioctl|compat_ioctl)[[:space:]]*='
  'atom_arc_firmware\.h'
)
for pattern in "${forbidden_source_patterns[@]}"; do
  if grep -En "${pattern}" "${source_file}"; then
    echo "STAGE0_POLICY_BLOCKED: active source contains a Stage 1+ path." >&2
    exit 1
  fi
done

claim_line=$(grep -n 'pci_request_regions[[:space:]]*(' "${source_file}" | cut -d: -f1)
dma_line=$(grep -n 'dma_set_mask_and_coherent[[:space:]]*(' "${source_file}" | cut -d: -f1)
map_line=$(grep -n 'pci_iomap[[:space:]]*(' "${source_file}" | cut -d: -f1)
if [[ -z ${claim_line} || -z ${dma_line} || -z ${map_line} ||
      ${claim_line} -ge ${dma_line} || ${dma_line} -ge ${map_line} ]]; then
  echo "STAGE0_POLICY_BLOCKED: required order is claim regions -> DMA mask -> BAR0 map." >&2
  exit 1
fi

if [[ -n ${module} ]]; then
  if [[ ! -f ${module} ]]; then
    echo "STAGE0_POLICY_BLOCKED: module does not exist: ${module}" >&2
    exit 1
  fi
  for tool in modinfo nm; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
      echo "STAGE0_POLICY_BLOCKED: required module-inspection tool is missing: ${tool}." >&2
      exit 1
    fi
  done
  if ! firmware_metadata=$(modinfo -F firmware "${module}") ||
     ! parameter_metadata=$(modinfo -F parm "${module}"); then
    echo "STAGE0_POLICY_BLOCKED: modinfo could not inspect the exact module." >&2
    exit 1
  fi
  if [[ -n ${firmware_metadata} || -n ${parameter_metadata} ]]; then
    echo "STAGE0_POLICY_BLOCKED: module exposes firmware metadata or parameters." >&2
    exit 1
  fi
  if ! undefined_symbols=$(nm -u "${module}"); then
    echo "STAGE0_POLICY_BLOCKED: nm could not inspect the exact module." >&2
    exit 1
  fi
  if grep -Eq \
    'request_firmware|release_firmware|(^|[[:space:]])(read[bwlq]|write[bwlq]|ioread|iowrite|dma_alloc|dma_map|dmaengine_|pci_alloc_irq_vectors|request_irq|request_threaded_irq|devm_request_irq|pci_reset|reset_control_|pci_set_master|drm_dev_register|misc_register|cdev_add|register_chrdev)' \
    <<<"${undefined_symbols}"; then
    echo "STAGE0_POLICY_BLOCKED: module imports a Stage 1+ symbol." >&2
    exit 1
  fi
fi

echo "ATOM_ARC_STAGE0_SOURCE_POLICY=PASS"
