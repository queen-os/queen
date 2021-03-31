arch := aarch64
board := virt
qemu := qemu-system-$(arch)
objcopy := aarch64-none-elf-objcopy
objdump := aarch64-none-elf-objdump
target := aarch64-unknown-none-softfloat
target_cpu := cortex-a72
mode := release
rust_flags := -C link-arg=-Tkernel/src/arch/$(arch)/boot/link.ld -C target-cpu=$(target_cpu)
build_args := --target=$(target)
build_path := ../target/$(target)/$(mode)
kernel := $(build_path)/queen-core
kernel_image := $(kernel).bin
smp := 4
qemu_opts := \
	-M $(board),gic-version=2 \
	-m 1G \
	-cpu $(target_cpu) \
	-smp $(smp) \
	-serial stdio -display none \
	-kernel $(kernel_image)
debug := 0
LOG := info

ifeq ($(mode), debug)
qemu_opts += -s -S
else
build_args += --release
endif

.PHONY: run build justrun clean

run: image justrun

build:
	RUSTFLAGS="$(rust_flags)" cargo rustc ${build_args}

image: build
	$(objcopy) -O binary --strip-all $(kernel) $(kernel_image)

justrun:
	$(qemu) $(qemu_opts)

clean:
	rm -rf ../target

gdb:
	gdb-multiarch -x release.gdb

dts:
	$(qemu) -s -M virt,dumpdtb=../target/virt.dtb,gic-version=2,its=off -m 1G -cpu $(target_cpu) -smp $(smp) -serial stdio -display none
	dtc -I dtb -O dts ../target/virt.dtb -o ../target/virt.dts

asm:
	$(objdump) --disassemble-all $(kernel) | less