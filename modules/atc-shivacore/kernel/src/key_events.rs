//! ENTWURF (nicht verdrahtet): Tastenereignisse vom Kernel zum Abonnenten.
//!
//! Bisher endet jede Taste im Bootlog. Der naechste Schritt waere ein Kanal zum
//! Userspace — und dafuer braucht es keine neue Syscall-Nummer: `IpcReceive`
//! existiert, ein Channel mit `READ`-Capability auch. Dieses Modul ist der
//! Prototyp dazu, mit Tests gegen die echte IPC-Schicht. Es ist absichtlich
//! NICHT im Bootpfad: welche Rechte, welche Puffergroesse und wer den Kanal
//! besitzt, sind Spec-Entscheidungen und stehen nicht mir zu.
//!
//! Die Richtung ist bewusst einseitig: der Kernel besitzt den Kanal und sendet,
//! der Prozess bekommt nur `READ`. Wuerde der Prozess `WRITE` halten, koennte er
//! sich seine eigenen Tastenereignisse in die Warteschlange legen — die
//! Eingabe waere dann nicht mehr die Eingabe.

use crate::ats1000::Pid;
use crate::capability::{CapabilityError, CapabilityTable, Rights};
use crate::ipc::{ChannelId, IpcError, IpcSubsystem};

/// Der Kernel als Absender.
pub const KERNEL: Pid = Pid(0);

/// Oeffnet den Tastenkanal: der Kernel besitzt ihn, der Prozess darf lesen.
pub fn open_key_channel(
    caps: &mut CapabilityTable,
    ipc: &mut IpcSubsystem,
    subscriber: Pid,
    capacity: usize,
) -> Result<ChannelId, CapabilityError> {
    let channel = ipc.create_channel(caps, KERNEL, capacity);
    ipc.grant_access(caps, KERNEL, channel, subscriber, Rights::READ)?;
    Ok(channel)
}

/// Stellt ein Tastenereignis zu. Der Scancode geht roh raus, die Uebersetzung
/// gehoert zum Prozess: der Kernel kennt kein Tastaturlayout.
pub fn deliver_key(
    ipc: &mut IpcSubsystem,
    caps: &CapabilityTable,
    channel: ChannelId,
    scancode: u8,
) -> Result<(), IpcError> {
    ipc.send(caps, KERNEL, channel, alloc::vec![scancode])
}

/// Holt das aelteste Ereignis ab (`None`, wenn nichts wartet).
pub fn read_key(
    ipc: &mut IpcSubsystem,
    caps: &CapabilityTable,
    subscriber: Pid,
    channel: ChannelId,
) -> Result<Option<u8>, IpcError> {
    match ipc.recv(caps, subscriber, channel) {
        Ok(message) => Ok(message.data.first().copied()),
        Err(IpcError::ChannelEmpty) => Ok(None),
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(capacity: usize) -> (CapabilityTable, IpcSubsystem, ChannelId, Pid) {
        let mut caps = CapabilityTable::new();
        let mut ipc = IpcSubsystem::new();
        let subscriber = Pid(1);
        let channel = open_key_channel(&mut caps, &mut ipc, subscriber, capacity).expect("Kanal");
        (caps, ipc, channel, subscriber)
    }

    #[test]
    fn a_key_reaches_the_subscriber_with_the_kernel_as_sender() {
        let (caps, mut ipc, channel, subscriber) = setup(8);
        deliver_key(&mut ipc, &caps, channel, 0x1E).expect("Zustellung");
        let message = ipc.recv(&caps, subscriber, channel).expect("Empfang");
        assert_eq!(message.data, alloc::vec![0x1E]);
        assert_eq!(
            message.sender, KERNEL,
            "Absender ist der Kernel, nicht der Prozess"
        );
    }

    #[test]
    fn the_subscriber_cannot_write_into_its_own_queue() {
        // Der eigentliche Grund fuer die Eigentuemerwahl: nur lesen, nicht
        // einspeisen. Sonst koennte ein Prozess seine eigene Eingabe faelschen.
        let (caps, mut ipc, channel, subscriber) = setup(8);
        let attempt = ipc.send(&caps, subscriber, channel, alloc::vec![0x41]);
        assert_eq!(attempt, Err(IpcError::NoWriteCapability));
    }

    #[test]
    fn the_kernel_does_not_need_a_second_channel() {
        let (caps, mut ipc, channel, _) = setup(8);
        deliver_key(&mut ipc, &caps, channel, 0x30).expect("erste");
        deliver_key(&mut ipc, &caps, channel, 0x31).expect("zweite");
        assert_eq!(ipc.channel_count(), 1);
        assert_eq!(ipc.pending_messages(channel), Some(2));
    }

    #[test]
    fn scancodes_keep_their_order() {
        let (caps, mut ipc, channel, subscriber) = setup(8);
        for scancode in [0x1E, 0x9E, 0x1F, 0x9F] {
            deliver_key(&mut ipc, &caps, channel, scancode).expect("Zustellung");
        }
        let mut seen = alloc::vec::Vec::new();
        while let Ok(Some(scancode)) = read_key(&mut ipc, &caps, subscriber, channel) {
            seen.push(scancode);
        }
        assert_eq!(seen, alloc::vec![0x1E, 0x9E, 0x1F, 0x9F]);
    }

    #[test]
    fn a_full_queue_refuses_instead_of_blocking_the_handler() {
        // Im Interrupt-Pfad darf nichts warten. Ein voller Puffer ist deshalb
        // ein Fehler, den der Handler zaehlt, und kein Wartezeitpunkt.
        let (caps, mut ipc, channel, _) = setup(2);
        deliver_key(&mut ipc, &caps, channel, 0x1E).expect("erste");
        deliver_key(&mut ipc, &caps, channel, 0x1F).expect("zweite");
        assert_eq!(
            deliver_key(&mut ipc, &caps, channel, 0x20),
            Err(IpcError::ChannelFull)
        );
    }

    #[test]
    fn an_empty_queue_reports_nothing_waiting() {
        let (caps, mut ipc, channel, subscriber) = setup(4);
        assert_eq!(read_key(&mut ipc, &caps, subscriber, channel), Ok(None));
    }

    #[test]
    fn a_foreign_process_cannot_read_the_queue() {
        let (caps, mut ipc, channel, _) = setup(4);
        let intruder = Pid(7);
        deliver_key(&mut ipc, &caps, channel, 0x1E).expect("Zustellung");
        assert_eq!(
            ipc.recv(&caps, intruder, channel),
            Err(IpcError::NoReadCapability)
        );
    }

    #[test]
    fn closing_the_channel_ends_delivery() {
        let (mut caps, mut ipc, channel, subscriber) = setup(4);
        ipc.close_channel(&mut caps, KERNEL, channel);
        assert_eq!(
            deliver_key(&mut ipc, &caps, channel, 0x1E),
            Err(IpcError::ChannelClosed)
        );
        assert_eq!(
            read_key(&mut ipc, &caps, subscriber, channel),
            Err(IpcError::ChannelClosed)
        );
    }
}
