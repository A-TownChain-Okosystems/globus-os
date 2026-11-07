// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore Boot-Image-Builder.
// Nimmt den fertig gebauten ShivaCore-Kernel-ELF (Argument 1) und erzeugt
// daraus bootfähige BIOS- und UEFI-Disk-Images per `bootloader` 0.11.
// Übernimmt NUR das reine Boot-Protokoll — kein Betriebssystem-Code, kein Linux.

use std::path::PathBuf;

fn main() {
    let kernel_path = std::env::args()
        .nth(1)
        .expect("Nutzung: boot <kernel-elf> <ausgabe-verzeichnis> [ramdisk-elf]");
    let out_dir = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    std::fs::create_dir_all(&out_dir).unwrap();

    // Optionales drittes Argument: ein Programm, das als Ramdisk in das Image
    // gelegt wird. Ohne es entsteht ein Image ohne Nutzlast, wie bisher.
    let ramdisk = std::env::args().nth(3).map(PathBuf::from);
    if let Some(path) = &ramdisk {
        let size = std::fs::metadata(path)
            .unwrap_or_else(|e| panic!("Ramdisk {} nicht lesbar: {}", path.display(), e))
            .len();
        println!("Ramdisk: {} ({} Bytes)", path.display(), size);
    }

    let bios_path = out_dir.join("shivacore-bios.img");
    let mut bios = bootloader::BiosBoot::new(&PathBuf::from(&kernel_path));
    if let Some(path) = &ramdisk {
        bios.set_ramdisk(path);
    }
    bios.create_disk_image(&bios_path)
        .expect("BIOS-Image-Erstellung fehlgeschlagen");
    println!("BIOS-Image: {}", bios_path.display());

    let uefi_path = out_dir.join("shivacore-uefi.img");
    let mut uefi = bootloader::UefiBoot::new(&PathBuf::from(&kernel_path));
    if let Some(path) = &ramdisk {
        uefi.set_ramdisk(path);
    }
    uefi.create_disk_image(&uefi_path)
        .expect("UEFI-Image-Erstellung fehlgeschlagen");
    println!("UEFI-Image: {}", uefi_path.display());
}
