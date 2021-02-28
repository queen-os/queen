arch := aarch64
board := virt
qemu := qemu-system-$(arch)
target := aarch64-unknown-none-softfloat
target_cpu := cortex-a72
mode := release
rust_flags := -C link-arg=-Tkernel/src/arch/$(arch)/boot/link.ld -C target-cpu=$(target_cpu)
build_args := --target=$(target)
build_path := ../target/$(target)/$(mode)
kernel := $(build_path)/queen-core
smp := 4
qemu_opts := \
	-s \
	-M $(board),gic-version=2 \
	-m 1G \
	-cpu $(target_cpu) \
	-smp $(smp) \
	-serial stdio -display none \
	-kernel $(kernel)

LOG := info

ifeq ($(mode), release)
build_args += --release
endif

.PHONY: run build justrun clean

run: build justrun

build:
	RUSTFLAGS="$(rust_flags)" cargo rustc ${build_args}

justrun:
	$(qemu) $(qemu_opts)

clean:
	rm -rf ../target

gdb:
	gdb-multiarch ../target/aarch64-unknown-none-softfloat/release/queen-core

dumpdts:
	$(qemu) -s -M virt,dumpdtb=virt.dtb,gic-version=3,its=off -m 1G -cpu $(target_cpu) -smp $(smp) -serial stdio -display none
	dtc virt.dtb