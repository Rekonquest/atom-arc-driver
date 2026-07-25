obj-m += atom_arc.o
atom_arc-y := atom_arc_main.o

ccflags-y += -Werror

.PHONY: all clean policy sparse

KERNEL_RELEASE ?= $(shell uname -r)
KERNEL_BUILD ?= /lib/modules/$(KERNEL_RELEASE)/build
KERNEL_CC ?= x86_64-linux-gnu-gcc-13
KBUILD_ARGS := M=$(CURDIR) W=1 CC=$(KERNEL_CC) CONFIG_DEBUG_INFO_BTF_MODULES=

all: policy
	$(MAKE) -C $(KERNEL_BUILD) $(KBUILD_ARGS) modules

policy:
	bash ./check-stage0-policy.sh

sparse: policy
	$(MAKE) -C $(KERNEL_BUILD) $(KBUILD_ARGS) C=2 CHECK=sparse CF=-D__CHECK_ENDIAN__ modules

clean:
	$(MAKE) -C $(KERNEL_BUILD) M=$(CURDIR) clean
