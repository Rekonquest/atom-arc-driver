/* SPDX-License-Identifier: MIT */
#ifndef ATOM_ARC_FIRMWARE_H
#define ATOM_ARC_FIRMWARE_H

#include <linux/device.h>
#include <linux/types.h>

#define ATOM_ARC_GUC_FIRMWARE "i915/dg2_guc_70.bin"
#define ATOM_ARC_HUC_FIRMWARE "i915/dg2_huc_gsc.bin"

struct atom_arc_fw_version {
	u16 major;
	u16 minor;
	u16 patch;
	u16 build;
};

struct atom_arc_firmware_info {
	struct atom_arc_fw_version guc;
	struct atom_arc_fw_version guc_submission;
	struct atom_arc_fw_version huc;
	u32 huc_security_version;
	size_t guc_bytes;
	size_t huc_bytes;
	bool guc_production;
	bool blobs_validated;
};

int atom_arc_firmware_validate(struct device *dev,
			       struct atom_arc_firmware_info *info);

#endif
