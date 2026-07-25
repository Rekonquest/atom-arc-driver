// SPDX-License-Identifier: MIT
/*
 * Independent, fail-closed Intel Arc A380 Stage 0 PCI probe.
 *
 * Stage 0 proves ownership plumbing only. It performs no GPU MMIO reads or
 * writes, firmware loading, DMA, IRQ allocation, reset, or command submission.
 */

#include <linux/device.h>
#include <linux/dma-mapping.h>
#include <linux/io.h>
#include <linux/module.h>
#include <linux/pci.h>

#define ATOM_ARC_VENDOR_ID 0x8086
#define ATOM_ARC_A380_DEVICE_ID 0x56a5
#define ATOM_ARC_DRIVER_NAME "atom_arc"
#define ATOM_ARC_DMA_ADDRESS_BITS 46U

struct atom_arc_device {
	struct pci_dev *pdev;
	void __iomem *mmio;
	resource_size_t bar0_bytes;
	resource_size_t bar2_bytes;
	u16 command_after_lock;
	bool dma_mask_configured;
	bool regions_claimed;
};

static void atom_arc_release_regions(void *data)
{
	struct atom_arc_device *arc = data;

	if (arc->mmio) {
		pci_iounmap(arc->pdev, arc->mmio);
		arc->mmio = NULL;
	}
	if (arc->regions_claimed) {
		pci_release_regions(arc->pdev);
		arc->regions_claimed = false;
	}
}

static struct atom_arc_device *atom_arc_from_dev(struct device *dev)
{
	return pci_get_drvdata(to_pci_dev(dev));
}

static ssize_t architecture_show(struct device *dev,
				 struct device_attribute *attribute, char *buffer)
{
	(void)dev;
	(void)attribute;
	return sysfs_emit(buffer, "standalone-linux-pci\n");
}
static DEVICE_ATTR_RO(architecture);

static ssize_t stage_show(struct device *dev,
			  struct device_attribute *attribute, char *buffer)
{
	(void)dev;
	(void)attribute;
	return sysfs_emit(buffer, "0\n");
}
static DEVICE_ATTR_RO(stage);

static ssize_t driver_state_show(struct device *dev,
				 struct device_attribute *attribute, char *buffer)
{
	(void)dev;
	(void)attribute;
	return sysfs_emit(buffer, "probe-only-locked\n");
}
static DEVICE_ATTR_RO(driver_state);

static ssize_t pci_bdf_show(struct device *dev,
			    struct device_attribute *attribute, char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%04x:%02x:%02x.%u\n",
			  pci_domain_nr(pdev->bus), pdev->bus->number,
			  PCI_SLOT(pdev->devfn), PCI_FUNC(pdev->devfn));
}
static DEVICE_ATTR_RO(pci_bdf);

static ssize_t pci_identity_show(struct device *dev,
				 struct device_attribute *attribute, char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%04x:%04x\n", pdev->vendor, pdev->device);
}
static DEVICE_ATTR_RO(pci_identity);

static ssize_t subsystem_identity_show(struct device *dev,
				       struct device_attribute *attribute,
				       char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%04x:%04x\n", pdev->subsystem_vendor,
			  pdev->subsystem_device);
}
static DEVICE_ATTR_RO(subsystem_identity);

static ssize_t revision_show(struct device *dev,
			     struct device_attribute *attribute, char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "0x%02x\n", pdev->revision);
}
static DEVICE_ATTR_RO(revision);

static ssize_t class_code_show(struct device *dev,
			       struct device_attribute *attribute, char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "0x%06x\n", pdev->class);
}
static DEVICE_ATTR_RO(class_code);

static ssize_t bar0_start_show(struct device *dev,
			       struct device_attribute *attribute, char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "0x%llx\n",
			  (unsigned long long)pci_resource_start(pdev, 0));
}
static DEVICE_ATTR_RO(bar0_start);

static ssize_t bar0_bytes_show(struct device *dev,
			       struct device_attribute *attribute, char *buffer)
{
	struct atom_arc_device *arc = atom_arc_from_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%llu\n",
			  (unsigned long long)arc->bar0_bytes);
}
static DEVICE_ATTR_RO(bar0_bytes);

static ssize_t bar2_start_show(struct device *dev,
			       struct device_attribute *attribute, char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);
	resource_size_t start = 0;

	(void)attribute;
	if (pci_resource_flags(pdev, 2) & IORESOURCE_MEM)
		start = pci_resource_start(pdev, 2);
	return sysfs_emit(buffer, "0x%llx\n", (unsigned long long)start);
}
static DEVICE_ATTR_RO(bar2_start);

static ssize_t bar2_bytes_show(struct device *dev,
			       struct device_attribute *attribute, char *buffer)
{
	struct atom_arc_device *arc = atom_arc_from_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%llu\n",
			  (unsigned long long)arc->bar2_bytes);
}
static DEVICE_ATTR_RO(bar2_bytes);

static ssize_t dma_address_bits_show(struct device *dev,
				     struct device_attribute *attribute,
				     char *buffer)
{
	struct atom_arc_device *arc = atom_arc_from_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%u\n",
			  arc->dma_mask_configured ? ATOM_ARC_DMA_ADDRESS_BITS : 0);
}
static DEVICE_ATTR_RO(dma_address_bits);

static ssize_t regions_claimed_show(struct device *dev,
				    struct device_attribute *attribute,
				    char *buffer)
{
	struct atom_arc_device *arc = atom_arc_from_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%u\n", arc->regions_claimed);
}
static DEVICE_ATTR_RO(regions_claimed);

static ssize_t mmio_mapped_show(struct device *dev,
				struct device_attribute *attribute, char *buffer)
{
	struct atom_arc_device *arc = atom_arc_from_dev(dev);

	(void)attribute;
	return sysfs_emit(buffer, "%u\n", arc->mmio != NULL);
}
static DEVICE_ATTR_RO(mmio_mapped);

static ssize_t mmio_read_count_show(struct device *dev,
				    struct device_attribute *attribute,
				    char *buffer)
{
	(void)dev;
	(void)attribute;
	return sysfs_emit(buffer, "0\n");
}
static DEVICE_ATTR_RO(mmio_read_count);

static ssize_t bus_master_enabled_show(struct device *dev,
				       struct device_attribute *attribute,
				       char *buffer)
{
	struct pci_dev *pdev = to_pci_dev(dev);
	u16 command;
	int ret;

	(void)attribute;
	ret = pci_read_config_word(pdev, PCI_COMMAND, &command);
	if (ret)
		return -EIO;
	return sysfs_emit(buffer, "%u\n", !!(command & PCI_COMMAND_MASTER));
}
static DEVICE_ATTR_RO(bus_master_enabled);

static ssize_t firmware_stage_show(struct device *dev,
				   struct device_attribute *attribute,
				   char *buffer)
{
	(void)dev;
	(void)attribute;
	return sysfs_emit(buffer, "not-started\n");
}
static DEVICE_ATTR_RO(firmware_stage);

static struct attribute *atom_arc_attributes[] = {
	&dev_attr_architecture.attr,
	&dev_attr_stage.attr,
	&dev_attr_driver_state.attr,
	&dev_attr_pci_bdf.attr,
	&dev_attr_pci_identity.attr,
	&dev_attr_subsystem_identity.attr,
	&dev_attr_revision.attr,
	&dev_attr_class_code.attr,
	&dev_attr_bar0_start.attr,
	&dev_attr_bar0_bytes.attr,
	&dev_attr_bar2_start.attr,
	&dev_attr_bar2_bytes.attr,
	&dev_attr_dma_address_bits.attr,
	&dev_attr_regions_claimed.attr,
	&dev_attr_mmio_mapped.attr,
	&dev_attr_mmio_read_count.attr,
	&dev_attr_bus_master_enabled.attr,
	&dev_attr_firmware_stage.attr,
	NULL,
};

static const struct attribute_group atom_arc_attribute_group = {
	.name = "atom_arc",
	.attrs = atom_arc_attributes,
};

static const struct attribute_group *atom_arc_attribute_groups[] = {
	&atom_arc_attribute_group,
	NULL,
};

static int atom_arc_probe(struct pci_dev *pdev,
			  const struct pci_device_id *id)
{
	struct atom_arc_device *arc;
	u16 pci_command;
	int ret;

	(void)id;
	if (!(pci_resource_flags(pdev, 0) & IORESOURCE_MEM) ||
	    !pci_resource_len(pdev, 0))
		return dev_err_probe(&pdev->dev, -ENODEV,
				     "A380 BAR0 is not a memory resource\n");

	arc = devm_kzalloc(&pdev->dev, sizeof(*arc), GFP_KERNEL);
	if (!arc)
		return -ENOMEM;
	arc->pdev = pdev;
	arc->bar0_bytes = pci_resource_len(pdev, 0);
	if (pci_resource_flags(pdev, 2) & IORESOURCE_MEM)
		arc->bar2_bytes = pci_resource_len(pdev, 2);
	pci_set_drvdata(pdev, arc);

	ret = pcim_enable_device(pdev);
	if (ret)
		return dev_err_probe(&pdev->dev, ret,
				     "failed to enable A380 memory resources\n");

	/*
	 * Establish the DMA lock before mapping MMIO. The command register may
	 * retain MASTER from firmware or a previous owner.
	 */
	pci_clear_master(pdev);
	ret = pci_read_config_word(pdev, PCI_COMMAND, &pci_command);
	if (ret)
		return dev_err_probe(&pdev->dev, -EIO,
				     "failed to read back the PCI command register\n");
	if (pci_command & PCI_COMMAND_MASTER)
		return dev_err_probe(&pdev->dev, -EIO,
				     "PCI bus mastering remained enabled after the security lock\n");
	arc->command_after_lock = pci_command;

	ret = pci_request_regions(pdev, ATOM_ARC_DRIVER_NAME);
	if (ret)
		return dev_err_probe(&pdev->dev, ret,
				     "failed to claim A380 PCI regions\n");
	arc->regions_claimed = true;
	ret = devm_add_action_or_reset(&pdev->dev, atom_arc_release_regions,
				       arc);
	if (ret)
		return ret;

	ret = dma_set_mask_and_coherent(&pdev->dev,
					DMA_BIT_MASK(ATOM_ARC_DMA_ADDRESS_BITS));
	if (ret)
		return dev_err_probe(&pdev->dev, ret,
				     "A380 requires a 46-bit coherent DMA mask\n");
	arc->dma_mask_configured = true;

	/*
	 * DG2 predates the GMD_ID discovery path used by newer Xe platforms.
	 * Stage 0 therefore identifies this exact board only from the safe PCI
	 * configuration fields exposed below and never dereferences BAR0.
	 */
	arc->mmio = pci_iomap(pdev, 0, 0);
	if (!arc->mmio)
		return dev_err_probe(&pdev->dev, -ENODEV,
				     "A380 BAR0 mapping is unavailable\n");

	dev_info(&pdev->dev,
		 "Stage 0 probe-only: %04x:%04x subsystem %04x:%04x rev %#02x class %#06x\n",
		 pdev->vendor, pdev->device, pdev->subsystem_vendor,
		 pdev->subsystem_device, pdev->revision, pdev->class);
	dev_info(&pdev->dev,
		 "BAR0 start=%#llx bytes=%llu; BAR2 start=%#llx bytes=%llu; DMA=%u-bit\n",
		 (unsigned long long)pci_resource_start(pdev, 0),
		 (unsigned long long)arc->bar0_bytes,
		 (unsigned long long)pci_resource_start(pdev, 2),
		 (unsigned long long)arc->bar2_bytes,
		 ATOM_ARC_DMA_ADDRESS_BITS);
	dev_info(&pdev->dev,
		 "BAR0 mapped without MMIO access; firmware, IRQs, DMA, reset, and submission are not started; PCI command=%#x\n",
		 arc->command_after_lock);
	return 0;
}

static void atom_arc_remove(struct pci_dev *pdev)
{
	pci_clear_master(pdev);
	dev_info(&pdev->dev,
		 "Stage 0 detached; bus mastering remains disabled\n");
}

static void atom_arc_shutdown(struct pci_dev *pdev)
{
	pci_clear_master(pdev);
}

static const struct pci_device_id atom_arc_pci_ids[] = {
	{ PCI_DEVICE(ATOM_ARC_VENDOR_ID, ATOM_ARC_A380_DEVICE_ID) },
	{ }
};
MODULE_DEVICE_TABLE(pci, atom_arc_pci_ids);

static struct pci_driver atom_arc_pci_driver = {
	.name = ATOM_ARC_DRIVER_NAME,
	.id_table = atom_arc_pci_ids,
	.probe = atom_arc_probe,
	.remove = atom_arc_remove,
	.shutdown = atom_arc_shutdown,
	.driver = {
		.dev_groups = atom_arc_attribute_groups,
	},
};

static int __init atom_arc_init(void)
{
	return pci_register_driver(&atom_arc_pci_driver);
}

static void __exit atom_arc_exit(void)
{
	pci_unregister_driver(&atom_arc_pci_driver);
}

module_init(atom_arc_init);
module_exit(atom_arc_exit);

MODULE_AUTHOR("Atom Arc contributors");
MODULE_DESCRIPTION("Independent fail-closed Intel Arc A380 Stage 0 PCI probe");
MODULE_LICENSE("Dual MIT/GPL");
