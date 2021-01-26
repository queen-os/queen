# Queen OS

鲲鹏920使用ACPI提供硬件信息，之前使用Device Tree。
Rust写Arm64架构的UEFI程序有问题。
QEMU virt board，直接运行ELF，使用Device Tree。
GDB 调试内核。

## Boot
* 建立临时页表，启动MMU
* 初始化 logging 模块，可以使用 `println!`、`info!()`、`error!()` 等宏。
* 唤醒其他CPU(PSCI)
* 初始化中断
* 初始化内存管理，包括物理页帧分配器与内核堆分配器，建立一个新的页表重新映射内核