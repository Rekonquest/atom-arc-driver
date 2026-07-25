// SPDX-License-Identifier: MIT
/*
 * Host-side validation for the published Intel DG2 GuC CSS and HuC GSC/CPD
 * firmware container ABIs. Cryptographic authentication remains a hardware
 * responsibility and is never replaced by these structural checks.
 */

#include <asm/unaligned.h>
#include <linux/firmware.h>
#include <linux/overflow.h>
#include <linux/string.h>

#include "atom_arc_firmware.h"

#define ATOM_ARC_CSS_HEADER_BYTES 128U
#define ATOM_ARC_CSS_HEADER_DWORDS (ATOM_ARC_CSS_HEADER_BYTES / sizeof(u32))
#define ATOM_ARC_CSS_HEADER_SIZE_DW_OFFSET 4U
#define ATOM_ARC_CSS_SIZE_DW_OFFSET 24U
#define ATOM_ARC_CSS_RSA_KEY_DW_OFFSET 28U
#define ATOM_ARC_CSS_RSA_MODULUS_DW_OFFSET 32U
#define ATOM_ARC_CSS_RSA_EXPONENT_DW_OFFSET 36U
#define ATOM_ARC_CSS_SW_VERSION_OFFSET 64U
#define ATOM_ARC_CSS_SUBMISSION_VERSION_OFFSET 68U
#define ATOM_ARC_CSS_UKERNEL_INFO_OFFSET 124U
#define ATOM_ARC_CSS_BUILD_TYPE_SHIFT 2U
#define ATOM_ARC_CSS_BUILD_TYPE_MASK 0x3U
#define ATOM_ARC_CSS_BUILD_PRODUCTION 0U

#define ATOM_ARC_CPD_MARKER 0x44504324U
#define ATOM_ARC_CPD_HEADER_MIN_BYTES 20U
#define ATOM_ARC_CPD_ENTRY_BYTES 24U
#define ATOM_ARC_CPD_MAX_ENTRIES 128U
#define ATOM_ARC_CPD_OFFSET_MASK 0x01ffffffU
#define ATOM_ARC_CPD_HUFFMAN_BIT BIT(25)
#define ATOM_ARC_GSC_MANIFEST_BYTES 128U
#define ATOM_ARC_GSC_MANIFEST_VERSION_OFFSET 36U
#define ATOM_ARC_GSC_MANIFEST_SECURITY_OFFSET 44U
#define ATOM_ARC_GSC_MANIFEST_VENDOR_OFFSET 16U
#define ATOM_ARC_GSC_MANIFEST_HEADER_LENGTH_OFFSET 4U
#define ATOM_ARC_GSC_MANIFEST_SIZE_OFFSET 24U
#define ATOM_ARC_INTEL_VENDOR_ID 0x8086U

struct atom_arc_cpd_item {
	size_t offset;
	size_t length;
};

static struct atom_arc_fw_version atom_arc_decode_css_version(const u8 *data)
{
	u32 value = get_unaligned_le32(data);

	return (struct atom_arc_fw_version) {
		.major = (value >> 16) & 0xff,
		.minor = (value >> 8) & 0xff,
		.patch = value & 0xff,
	};
}

static int atom_arc_validate_guc(struct device *dev,
				 const struct firmware *firmware,
				 struct atom_arc_firmware_info *info)
{
	const u8 *data = firmware->data;
	size_t header_size_dw;
	size_t size_dw;
	size_t key_size_dw;
	size_t modulus_size_dw;
	size_t exponent_size_dw;
	size_t non_ucode_header_dw;
	size_t ucode_dw;
	size_t ucode_bytes;
	size_t rsa_bytes;
	size_t minimum_bytes;
	u32 ukernel_info;
	u32 build_type;

	if (firmware->size < ATOM_ARC_CSS_HEADER_BYTES)
		return dev_err_probe(dev, -ENODATA,
				     "GuC CSS header is truncated (%zu bytes)\n",
				     firmware->size);

	header_size_dw = get_unaligned_le32(
		data + ATOM_ARC_CSS_HEADER_SIZE_DW_OFFSET);
	size_dw = get_unaligned_le32(data + ATOM_ARC_CSS_SIZE_DW_OFFSET);
	key_size_dw = get_unaligned_le32(
		data + ATOM_ARC_CSS_RSA_KEY_DW_OFFSET);
	modulus_size_dw = get_unaligned_le32(
		data + ATOM_ARC_CSS_RSA_MODULUS_DW_OFFSET);
	exponent_size_dw = get_unaligned_le32(
		data + ATOM_ARC_CSS_RSA_EXPONENT_DW_OFFSET);

	if (!key_size_dw || header_size_dw < key_size_dw ||
	    header_size_dw - key_size_dw < modulus_size_dw ||
	    header_size_dw - key_size_dw - modulus_size_dw < exponent_size_dw)
		return dev_err_probe(dev, -EPROTO,
				     "GuC CSS signature sizes are invalid\n");

	non_ucode_header_dw = header_size_dw - key_size_dw -
		modulus_size_dw - exponent_size_dw;
	if (non_ucode_header_dw != ATOM_ARC_CSS_HEADER_DWORDS)
		return dev_err_probe(dev, -EPROTO,
				     "GuC CSS header is %zu dwords, expected %zu\n",
				     non_ucode_header_dw,
				     ATOM_ARC_CSS_HEADER_DWORDS);
	if (size_dw < header_size_dw)
		return dev_err_probe(dev, -EPROTO,
				     "GuC CSS total size precedes its header\n");

	ucode_dw = size_dw - header_size_dw;
	if (check_mul_overflow(ucode_dw, sizeof(u32), &ucode_bytes) ||
	    check_mul_overflow(key_size_dw, sizeof(u32), &rsa_bytes) ||
	    check_add_overflow((size_t)ATOM_ARC_CSS_HEADER_BYTES,
			       ucode_bytes, &minimum_bytes) ||
	    check_add_overflow(minimum_bytes, rsa_bytes, &minimum_bytes))
		return dev_err_probe(dev, -EOVERFLOW,
				     "GuC CSS sizes overflow the host address space\n");
	if (minimum_bytes > firmware->size)
		return dev_err_probe(dev, -ENOEXEC,
				     "GuC CSS payload is truncated (%zu < %zu)\n",
				     firmware->size, minimum_bytes);

	info->guc = atom_arc_decode_css_version(
		data + ATOM_ARC_CSS_SW_VERSION_OFFSET);
	info->guc_submission = atom_arc_decode_css_version(
		data + ATOM_ARC_CSS_SUBMISSION_VERSION_OFFSET);
	ukernel_info = get_unaligned_le32(
		data + ATOM_ARC_CSS_UKERNEL_INFO_OFFSET);
	build_type = (ukernel_info >> ATOM_ARC_CSS_BUILD_TYPE_SHIFT) &
		ATOM_ARC_CSS_BUILD_TYPE_MASK;
	info->guc_production = build_type == ATOM_ARC_CSS_BUILD_PRODUCTION;

	if (info->guc.major != 70 || info->guc.minor < 29)
		return dev_err_probe(dev, -ENOEXEC,
				     "unsupported GuC release %u.%u.%u\n",
				     info->guc.major, info->guc.minor,
				     info->guc.patch);
	if (!info->guc_production)
		return dev_err_probe(dev, -EKEYREJECTED,
				     "refusing non-production GuC build type %u\n",
				     build_type);

	info->guc_bytes = firmware->size;
	return 0;
}

static bool atom_arc_cpd_name_matches(const u8 *entry, const char *name)
{
	size_t length = strlen(name);

	if (length > 12 || memcmp(entry, name, length))
		return false;
	return length == 12 || entry[length] == '\0';
}

static int atom_arc_find_cpd_item(struct device *dev, const u8 *data,
				  size_t size, size_t header_length,
				  u32 entry_count, const char *name,
				  struct atom_arc_cpd_item *item)
{
	u32 index;

	for (index = 0; index < entry_count; ++index) {
		const u8 *entry = data + header_length +
			(size_t)index * ATOM_ARC_CPD_ENTRY_BYTES;
		u32 encoded_offset;
		size_t offset;
		size_t length;

		if (!atom_arc_cpd_name_matches(entry, name))
			continue;
		encoded_offset = get_unaligned_le32(entry + 12);
		if (encoded_offset & ATOM_ARC_CPD_HUFFMAN_BIT)
			return dev_err_probe(dev, -ENOTSUPP,
					     "CPD item %s is Huffman compressed\n",
					     name);
		offset = encoded_offset & ATOM_ARC_CPD_OFFSET_MASK;
		length = get_unaligned_le32(entry + 16);
		if (offset > size || length > size - offset)
			return dev_err_probe(dev, -ENODATA,
					     "CPD item %s exceeds the HuC blob\n",
					     name);
		item->offset = offset;
		item->length = length;
		return 0;
	}

	return dev_err_probe(dev, -ENODATA, "CPD item %s is missing\n", name);
}

static int atom_arc_validate_huc(struct device *dev,
				 const struct firmware *firmware,
				 struct atom_arc_firmware_info *info)
{
	const u8 *data = firmware->data;
	u32 entry_count;
	size_t header_length;
	size_t entries_bytes;
	size_t directory_bytes;
	struct atom_arc_cpd_item manifest = { 0 };
	struct atom_arc_cpd_item code = { 0 };
	const u8 *manifest_data;
	size_t manifest_header_bytes;
	size_t manifest_bytes;
	u32 vendor;
	int ret;

	if (firmware->size < ATOM_ARC_CPD_HEADER_MIN_BYTES ||
	    get_unaligned_le32(data) != ATOM_ARC_CPD_MARKER)
		return dev_err_probe(dev, -EPROTO,
				     "HuC firmware has no DG2 CPD header\n");

	entry_count = get_unaligned_le32(data + 4);
	header_length = data[10];
	if (!entry_count || entry_count > ATOM_ARC_CPD_MAX_ENTRIES ||
	    header_length < ATOM_ARC_CPD_HEADER_MIN_BYTES)
		return dev_err_probe(dev, -EPROTO,
				     "HuC CPD directory metadata is invalid\n");
	if (check_mul_overflow((size_t)entry_count,
			       (size_t)ATOM_ARC_CPD_ENTRY_BYTES,
			       &entries_bytes) ||
	    check_add_overflow(header_length, entries_bytes, &directory_bytes) ||
	    directory_bytes > firmware->size)
		return dev_err_probe(dev, -ENODATA,
				     "HuC CPD directory is truncated\n");

	ret = atom_arc_find_cpd_item(dev, data, firmware->size,
				     header_length, entry_count, "HUCP.man",
				     &manifest);
	if (ret)
		return ret;
	ret = atom_arc_find_cpd_item(dev, data, firmware->size,
				     header_length, entry_count, "huc_fw",
				     &code);
	if (ret)
		return ret;
	if (manifest.length < ATOM_ARC_GSC_MANIFEST_BYTES || !code.length)
		return dev_err_probe(dev, -ENODATA,
				     "HuC manifest or firmware code is empty\n");

	manifest_data = data + manifest.offset;
	vendor = get_unaligned_le32(
		manifest_data + ATOM_ARC_GSC_MANIFEST_VENDOR_OFFSET);
	manifest_header_bytes = (size_t)get_unaligned_le32(
		manifest_data + ATOM_ARC_GSC_MANIFEST_HEADER_LENGTH_OFFSET) *
		sizeof(u32);
	manifest_bytes = (size_t)get_unaligned_le32(
		manifest_data + ATOM_ARC_GSC_MANIFEST_SIZE_OFFSET) *
		sizeof(u32);
	if (vendor != ATOM_ARC_INTEL_VENDOR_ID ||
	    manifest_header_bytes > manifest.length ||
	    manifest_bytes > manifest.length ||
	    manifest_header_bytes < ATOM_ARC_GSC_MANIFEST_BYTES)
		return dev_err_probe(dev, -EKEYREJECTED,
				     "HuC manifest identity or bounds are invalid\n");

	info->huc.major = get_unaligned_le16(
		manifest_data + ATOM_ARC_GSC_MANIFEST_VERSION_OFFSET);
	info->huc.minor = get_unaligned_le16(
		manifest_data + ATOM_ARC_GSC_MANIFEST_VERSION_OFFSET + 2);
	info->huc.patch = get_unaligned_le16(
		manifest_data + ATOM_ARC_GSC_MANIFEST_VERSION_OFFSET + 4);
	info->huc.build = get_unaligned_le16(
		manifest_data + ATOM_ARC_GSC_MANIFEST_VERSION_OFFSET + 6);
	info->huc_security_version = get_unaligned_le32(
		manifest_data + ATOM_ARC_GSC_MANIFEST_SECURITY_OFFSET);
	if (info->huc.major != 7 || info->huc.minor < 10 ||
	    !info->huc_security_version)
		return dev_err_probe(dev, -ENOEXEC,
				     "unsupported HuC release %u.%u.%u security %u\n",
				     info->huc.major, info->huc.minor,
				     info->huc.patch,
				     info->huc_security_version);

	info->huc_bytes = firmware->size;
	return 0;
}

int atom_arc_firmware_validate(struct device *dev,
			       struct atom_arc_firmware_info *info)
{
	const struct firmware *guc = NULL;
	const struct firmware *huc = NULL;
	int ret;

	memset(info, 0, sizeof(*info));
	ret = request_firmware_direct(&guc, ATOM_ARC_GUC_FIRMWARE, dev);
	if (ret)
		return dev_err_probe(dev, ret, "failed to load %s\n",
				     ATOM_ARC_GUC_FIRMWARE);
	ret = atom_arc_validate_guc(dev, guc, info);
	if (ret)
		goto out;

	ret = request_firmware_direct(&huc, ATOM_ARC_HUC_FIRMWARE, dev);
	if (ret) {
		dev_err_probe(dev, ret, "failed to load %s\n",
			      ATOM_ARC_HUC_FIRMWARE);
		goto out;
	}
	ret = atom_arc_validate_huc(dev, huc, info);
	if (ret)
		goto out;

	info->blobs_validated = true;
	dev_info(dev,
		 "validated production GuC %u.%u.%u and HuC %u.%u.%u.%u (SVN %u); hardware authentication still required\n",
		 info->guc.major, info->guc.minor, info->guc.patch,
		 info->huc.major, info->huc.minor, info->huc.patch,
		 info->huc.build, info->huc_security_version);
out:
	release_firmware(huc);
	release_firmware(guc);
	return ret;
}
