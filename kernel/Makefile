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
qemu_opts := \
	-M $(board) \
	-cpu $(target_cpu) \
	-serial stdio -display none \
	-kernel $(kernel)

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