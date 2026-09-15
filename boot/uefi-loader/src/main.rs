#![no_main]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::{mem, ptr, slice};
use uefi::{
    boot::{self, AllocateType, MemoryType},
    cstr16, entry,
    fs::FileSystem,
    helpers, println,
    table::cfg::ConfigTableEntry,
    system,
    Handle, Status,
};

const KERNEL_PATH: &uefi::CStr16 = cstr16!("\\EFI\\GLOBUS\\SHIVACORE.ELF");
const PAGE_SIZE: u64 = 4096;
const PT_LOAD: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub _reserved: u32,
    pub memory_map: *const u8,
    pub memory_map_size: usize,
    pub memory_descriptor_size: usize,
    pub memory_descriptor_version: u32,
    pub acpi_rsdp: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    ident: [u8; 16],
    typ: u16,
    machine: u16,
    version: u32,
    entry: u64,
    phoff: u64,
    shoff: u64,
    flags: u32,
    ehsize: u16,
    phentsize: u16,
    phnum: u16,
    shentsize: u16,
    shnum: u16,
    shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64ProgramHeader {
    typ: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

#[entry]
fn main() -> Status {
    if let Err(status) = run() {
        println!("GlobusOS UEFI loader failed: {:?}", status);
        return status;
    }
    Status::SUCCESS
}

fn run() -> Result<(), Status> {
    helpers::init().map_err(|e| e.status())?;
    println!("GlobusOS UEFI loader: starting");

    let image = boot::image_handle();
    let mut fs = FileSystem::new(boot::get_image_file_system(image).map_err(|e| e.status())?);
    let kernel = fs.read(KERNEL_PATH).map_err(|e| e.status())?;
    drop(fs);

    let (entry_point, loaded_start, loaded_end) = load_elf64(&kernel)?;
    println!("ShivaCore ELF loaded: {:#x}-{:#x}", loaded_start, loaded_end);

    let acpi_rsdp = find_acpi_rsdp();

    // All protocol handles and allocations using boot services must be quiesced
    // before ExitBootServices. The returned memory map is retained for the kernel.
    let memory_map = unsafe { boot::exit_boot_services(None) };
    let meta = memory_map.meta();
    let boot_info = BootInfo {
        magic: 0x4752_4F42_5553_4249,
        version: 1,
        _reserved: 0,
        memory_map: memory_map.buffer().as_ptr(),
        memory_map_size: meta.map_size,
        memory_descriptor_size: meta.desc_size,
        memory_descriptor_version: meta.desc_version,
        acpi_rsdp,
    };

    // The memory-map allocation is intentionally transferred to the kernel.
    let boot_info_ptr = &boot_info as *const BootInfo;
    mem::forget(memory_map);

    let entry: extern "sysv64" fn(*const BootInfo) -> ! = unsafe { mem::transmute(entry_point) };
    let _ = (loaded_start, loaded_end);
    entry(boot_info_ptr)
}

fn load_elf64(image: &[u8]) -> Result<(u64, u64, u64), Status> {
    if image.len() < mem::size_of::<Elf64Header>() {
        return Err(Status::LOAD_ERROR);
    }

    let header = unsafe { ptr::read_unaligned(image.as_ptr().cast::<Elf64Header>()) };
    if &header.ident[0..4] != b"\\x7fELF"
        || header.ident[4] != 2
        || header.ident[5] != 1
        || header.typ != ET_EXEC
        || header.machine != EM_X86_64
        || header.version != 1
        || header.phentsize as usize != mem::size_of::<Elf64ProgramHeader>()
    {
        return Err(Status::LOAD_ERROR);
    }

    let ph_end = header
        .phoff
        .checked_add((header.phnum as u64).checked_mul(header.phentsize as u64).ok_or(Status::LOAD_ERROR)?)
        .ok_or(Status::LOAD_ERROR)?;
    if ph_end as usize > image.len() {
        return Err(Status::LOAD_ERROR);
    }

    let mut loaded_start = u64::MAX;
    let mut loaded_end = 0u64;

    for index in 0..header.phnum as usize {
        let offset = header.phoff as usize + index * header.phentsize as usize;
        let ph = unsafe { ptr::read_unaligned(image.as_ptr().add(offset).cast::<Elf64ProgramHeader>()) };
        if ph.typ != PT_LOAD || ph.memsz == 0 {
            continue;
        }
        if ph.filesz > ph.memsz {
            return Err(Status::LOAD_ERROR);
        }
        let file_end = ph.offset.checked_add(ph.filesz).ok_or(Status::LOAD_ERROR)?;
        if file_end as usize > image.len() {
            return Err(Status::LOAD_ERROR);
        }

        let segment_start = ph.paddr & !(PAGE_SIZE - 1);
        let segment_offset = ph.paddr - segment_start;
        let total = segment_offset.checked_add(ph.memsz).ok_or(Status::LOAD_ERROR)?;
        let pages = total.div_ceil(PAGE_SIZE) as usize;
        boot::allocate_pages(
            AllocateType::Address(segment_start),
            if ph.flags & 1 != 0 { MemoryType::LOADER_CODE } else { MemoryType::LOADER_DATA },
            pages,
        )
        .map_err(|e| e.status())?;

        unsafe {
            let dst = ph.paddr as *mut u8;
            ptr::copy_nonoverlapping(image.as_ptr().add(ph.offset as usize), dst, ph.filesz as usize);
            ptr::write_bytes(dst.add(ph.filesz as usize), 0, (ph.memsz - ph.filesz) as usize);
        }

        loaded_start = loaded_start.min(segment_start);
        loaded_end = loaded_end.max(segment_start + pages as u64 * PAGE_SIZE);
    }

    if loaded_start == u64::MAX || header.entry < loaded_start || header.entry >= loaded_end {
        return Err(Status::LOAD_ERROR);
    }

    Ok((header.entry, loaded_start, loaded_end))
}

fn find_acpi_rsdp() -> u64 {
    let mut address = 0u64;
    system::with_config_table(|entries| {
        for entry in entries {
            if entry.guid == ConfigTableEntry::ACPI2_GUID || entry.guid == ConfigTableEntry::ACPI_GUID {
                address = entry.address as u64;
                if entry.guid == ConfigTableEntry::ACPI2_GUID {
                    break;
                }
            }
        }
    });
    address
}

#[allow(dead_code)]
fn _memory_map_slice(ptr: *const u8, size: usize) -> &'static [u8] {
    unsafe { slice::from_raw_parts(ptr, size) }
}
