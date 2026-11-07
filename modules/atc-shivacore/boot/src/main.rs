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

    // Ab dem dritten Argument: Programme fuer die Ramdisk, je als `name=pfad`.
    // Sie werden zu einem Verzeichnis gebuendelt (Format SRAM), damit der Kernel
    // nach Namen auswaehlen kann statt nur ein einzelnes ELF zu kennen.
    let programs: Vec<(String, Vec<u8>)> = std::env::args()
        .skip(3)
        .map(|argument| {
            let (name, path) = argument
                .split_once('=')
                .unwrap_or_else(|| panic!("Ramdisk-Eintrag ohne '=': {}", argument));
            let bytes = std::fs::read(path)
                .unwrap_or_else(|e| panic!("Ramdisk-Eintrag {} nicht lesbar: {}", path, e));
            (name.to_string(), bytes)
        })
        .collect();

    let ramdisk = if programs.is_empty() {
        None
    } else {
        let entries: Vec<(&str, &[u8])> = programs
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect();
        let image = shivacore::ramdisk::build(&entries)
            .unwrap_or_else(|error| panic!("Ramdisk-Verzeichnis nicht baubar: {:?}", error));
        let path = out_dir.join("ramdisk.bin");
        std::fs::write(&path, &image).expect("Ramdisk nicht schreibbar");
        println!(
            "Ramdisk: {} Programme ({} Bytes) -> {}",
            programs.len(),
            image.len(),
            path.display()
        );
        Some(path)
    };

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
