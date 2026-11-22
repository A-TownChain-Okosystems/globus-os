//! Durchgehender Speicherpfad ohne Hardware.
//!
//! Der Test verbindet die vier Bausteine, die bisher nur einzeln geprueft waren:
//! Seitenrahmen-Allokator, IOMMU-Abbildung, NVMe-Queue und Registerfenster des
//! Controllers. Er belegt, dass die Schnittstellen zusammenpassen, nicht dass
//! ein echtes Geraet funktioniert.

use globus_devices::DeviceId;
use globus_devices::dma_alloc::{FrameAllocator, PAGE_SIZE};
use globus_devices::iommu::{DmaDomainId, Iommu, SoftwareIommu};
use globus_devices::nvme::{NvmeCommand, NvmeCompletion, NvmeDmaBuffer, NvmeQueue, NvmeQueueId};
use globus_devices::nvme_controller::{
    AdminQueueAddresses, CC, CC_EN, CSTS, CSTS_RDY, ControllerState, NvmeController,
    NvmeControllerConfig, NvmeMmio,
};
use globus_devices::nvme_mmio::BarWindow;

const POOL_BASE: u64 = 0x1000_0000;
const IOVA_BASE: u64 = 0x4000;

fn config() -> NvmeControllerConfig {
    NvmeControllerConfig {
        admin_queue_depth: 32,
        io_queue_depth: 256,
        page_size: 4096,
    }
}

#[test]
fn buffer_travels_from_allocator_through_iommu_into_the_queue() {
    // 1. Physisch zusammenhaengender Puffer.
    let mut frames = FrameAllocator::new(POOL_BASE, 64).expect("Pool");
    let allocation = frames.allocate(2 * PAGE_SIZE).expect("zwei Seiten");
    assert_eq!(allocation.physical, POOL_BASE);
    assert_eq!(allocation.length, 2 * PAGE_SIZE);

    // 2. Abbildung im Geraeteadressraum.
    let mut iommu = SoftwareIommu::default();
    let domain = DmaDomainId(7);
    iommu.create_domain(domain).expect("Domain");
    iommu.assign_device(domain, DeviceId(1)).expect("Geraet");
    let region = allocation.region(IOVA_BASE, true);
    iommu.map(domain, region).expect("Abbildung");
    assert_eq!(iommu.mappings(domain).expect("Abfrage").len(), 1);

    // 3. Kommando mit genau diesem Puffer.
    let mut queue = NvmeQueue::new(NvmeQueueId(1), 8).expect("Queue");
    let command = NvmeCommand {
        opcode: 0x02,
        namespace: 1,
        data: Some(NvmeDmaBuffer {
            address: region.iova,
            length: allocation.length as u32,
        }),
        command_id: 1,
    };
    queue.submit(command).expect("Einreichen");
    let taken = queue.take_submission().expect("Abholung");
    assert_eq!(taken.command_id, 1);
    assert_eq!(taken.data.expect("Puffer").address, IOVA_BASE);

    // 4. Fertigmeldung schliesst das Kommando ab.
    queue
        .complete(NvmeCompletion {
            command_id: 1,
            status: 0,
            result: 0,
        })
        .expect("Fertigmeldung");
    assert_eq!(queue.poll_completion().expect("Abschluss").command_id, 1);
}

#[test]
fn controller_comes_up_against_the_register_window() {
    let mut mmio = BarWindow::new(4096).expect("Fenster");
    let mut controller = NvmeController::new(config()).expect("Controller");
    controller
        .configure_admin_queues(
            &mut mmio,
            AdminQueueAddresses {
                submission: POOL_BASE,
                completion: POOL_BASE + PAGE_SIZE,
            },
        )
        .expect("Admin-Queues");
    controller.enable(&mut mmio).expect("Enable");
    assert_eq!(mmio.read32(CC) & CC_EN, CC_EN);
    assert_eq!(controller.state(), ControllerState::Enabling);

    // Der Controller wird erst bereit, wenn die Hardware es meldet.
    mmio.write32(CSTS, CSTS_RDY);
    controller.poll_ready(&mmio).expect("Bereitschaft");
    assert_eq!(controller.state(), ControllerState::Ready);
}

#[test]
fn unusable_buffer_is_rejected_before_it_reaches_the_queue() {
    let mut queue = NvmeQueue::new(NvmeQueueId(2), 4).expect("Queue");
    let broken = NvmeCommand {
        opcode: 0x02,
        namespace: 1,
        data: Some(NvmeDmaBuffer {
            address: 0,
            length: 0,
        }),
        command_id: 2,
    };
    assert!(queue.submit(broken).is_err());

    // Auch eine Ueberlappung darf die IOMMU nicht durchlassen.
    let mut iommu = SoftwareIommu::default();
    let domain = DmaDomainId(9);
    iommu.create_domain(domain).expect("Domain");
    let mut frames = FrameAllocator::new(POOL_BASE, 8).expect("Pool");
    let a = frames.allocate(PAGE_SIZE).expect("Seite");
    iommu.map(domain, a.region(IOVA_BASE, true)).expect("erste");
    let b = frames.allocate(PAGE_SIZE).expect("Seite");
    assert!(iommu.map(domain, b.region(IOVA_BASE, true)).is_err());
}
