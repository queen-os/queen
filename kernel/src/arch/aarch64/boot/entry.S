.global _start
_start:
    mrs     x1, mpidr_el1   // Read Multiprocessor Affinity Register
    and     x1, x1, #3      // Clear all bits except [1:0], which hold core id
    cbz     x1, 2f          // Jump to label 2 if we are core 0
1:  wfe                     // Wait for event
    b       1b              // In case an event happened, jump back to 1
2:                          // If we are here, we are core0
    ldr     x1, =stack_top     // Load address of function "_start()"
    mov     sp, x1          // Set start of stack to before our code, aka first
                            // address before "_start()"
    bl      main_start    // Jump to the "main_start()" kernel function
    b       1b              // We should never reach here. But just in case,
                            // park this core aswell