// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// atc-bootloader — Custom bootloader for ShivaCore kernel
/// Multi-stage boot sequence state machine.
pub mod boot;
/// Entry point address the bootloader jumps to.
pub mod entry;
/// Boot info handed over to the kernel.
pub mod info;
/// Memory layout of the kernel image.
pub mod mmap;
