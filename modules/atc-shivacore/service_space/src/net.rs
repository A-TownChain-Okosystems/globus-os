// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// K-Sprint 12 — Netzwerk-Stack Foundation
// Kernel Layer | Chain-ID 658467
// NetworkDevice Trait, Ethernet-Frames, ARP-Table, IPv4/UDP-Parsing.
// Trait-basiert: virtio-net/E1000 in Hardware, LoopbackDevice für Tests.
// ─────────────────────────────────────────────────────────────────────────

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

// ─── MAC-Adresse ────────────────────────────────────────────────────────────

/// MAC-Adresse als sechs Byte; die Anzeige schreibt sie in Hex mit Doppelpunkten.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Adresse aus sechs Byte in Übertragungsreihenfolge.
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        MacAddress([a, b, c, d, e, f])
    }

    /// Rundrufadresse FF:FF:FF:FF:FF:FF.
    pub fn broadcast() -> Self {
        MacAddress([0xFF; 6])
    }

    /// Nulladresse; sie steht für eine noch unbekannte Adresse.
    pub fn zero() -> Self {
        MacAddress([0; 6])
    }

    /// True, wenn alle sechs Byte gesetzt sind.
    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 6]
    }

    /// True, wenn alle sechs Byte 0 sind.
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 6]
    }
}

impl core::fmt::Display for MacAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

// ─── IPv4-Adresse ───────────────────────────────────────────────────────────

/// IPv4-Adresse als vier Byte; die Anzeige schreibt sie in Punktnotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    /// Adresse aus vier Byte in Netzwerkreihenfolge.
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Ipv4Address([a, b, c, d])
    }

    /// Nulladresse 0.0.0.0; sie steht für eine noch unbekannte Adresse.
    pub fn zero() -> Self {
        Ipv4Address([0; 4])
    }
    /// Rundrufadresse 255.255.255.255.
    pub fn broadcast() -> Self {
        Ipv4Address([0xFF; 4])
    }

    /// True, wenn alle vier Byte gesetzt sind.
    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 4]
    }
    /// True, wenn alle vier Byte 0 sind.
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }
}

impl core::fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

// ─── Ethernet-Frame ────────────────────────────────────────────────────────

/// Ethertyp der ARP-Nutzlast.
pub const ETH_TYPE_ARP: u16 = 0x0806;
/// Ethertyp der IPv4-Nutzlast.
pub const ETH_TYPE_IPV4: u16 = 0x0800;

/// Ethernet-Rahmen mit Ziel, Quelle, Typ und Nutzlast.
#[derive(Clone, Debug, PartialEq)]
pub struct EthernetFrame {
    /// Zieladresse des Rahmens.
    pub dst_mac: MacAddress,
    /// Quelladresse des Rahmens.
    pub src_mac: MacAddress,
    /// Typ der Nutzlast.
    pub ethertype: u16,
    /// Nutzlast des Rahmens.
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    /// Rahmen aus Ziel, Quelle, Typ und Nutzlast.
    pub fn new(dst: MacAddress, src: MacAddress, ethertype: u16, payload: Vec<u8>) -> Self {
        EthernetFrame {
            dst_mac: dst,
            src_mac: src,
            ethertype,
            payload,
        }
    }

    /// Serialisiert das Frame zu Bytes (dst[6] + src[6] + type[2] + payload).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(14 + self.payload.len());
        buf.extend_from_slice(&self.dst_mac.0);
        buf.extend_from_slice(&self.src_mac.0);
        buf.extend_from_slice(&self.ethertype.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserialisiert ein Frame aus Bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, NetworkError> {
        if data.len() < 14 {
            return Err(NetworkError::FrameTooShort);
        }
        let mut dst = [0u8; 6];
        dst.copy_from_slice(&data[0..6]);
        let mut src = [0u8; 6];
        src.copy_from_slice(&data[6..12]);
        let ethertype = u16::from_be_bytes([data[12], data[13]]);
        let payload = data[14..].to_vec();

        Ok(EthernetFrame {
            dst_mac: MacAddress(dst),
            src_mac: MacAddress(src),
            ethertype,
            payload,
        })
    }
}

// ─── ARP (Address Resolution Protocol) ──────────────────────────────────────

/// Eintrag der ARP-Tabelle zu einer IP-Adresse.
#[derive(Clone, Debug, PartialEq)]
pub struct ArpEntry {
    /// IP-Adresse, zu der der Eintrag gehört.
    pub ip: Ipv4Address,
    /// Zugehörige MAC-Adresse.
    pub mac: MacAddress,
    /// Zeitpunkt der Aufnahme; 0 bei dauerhaften Einträgen.
    pub timestamp: u64,
    /// True, wenn der Eintrag nicht abläuft.
    pub permanent: bool,
}

/// Zuordnung von IP-Adressen zu MAC-Adressen mit Ablaufzeit.
pub struct ArpTable {
    entries: Mutex<BTreeMap<Ipv4Address, ArpEntry>>,
    timeout_ns: u64,
}

impl ArpTable {
    /// Leere Tabelle mit Ablaufzeit in Nanosekunden.
    pub fn new(timeout_ns: u64) -> Self {
        ArpTable {
            entries: Mutex::new(BTreeMap::new()),
            timeout_ns,
        }
    }

    /// MAC-Adresse zu einer IP; None, wenn kein Eintrag vorliegt. Die Ablaufzeit prüft erst purge_expired.
    pub fn lookup(&self, ip: Ipv4Address) -> Option<MacAddress> {
        let entries = self.entries.lock();
        entries.get(&ip).map(|e| e.mac)
    }

    /// Trägt einen Eintrag mit Zeitstempel ein und überschreibt einen vorhandenen.
    pub fn insert(&self, ip: Ipv4Address, mac: MacAddress, timestamp: u64) {
        let mut entries = self.entries.lock();
        entries.insert(
            ip,
            ArpEntry {
                ip,
                mac,
                timestamp,
                permanent: false,
            },
        );
    }

    /// Trägt einen dauerhaften Eintrag ein, der nicht abläuft.
    pub fn insert_permanent(&self, ip: Ipv4Address, mac: MacAddress) {
        let mut entries = self.entries.lock();
        entries.insert(
            ip,
            ArpEntry {
                ip,
                mac,
                timestamp: 0,
                permanent: true,
            },
        );
    }

    /// Entfernt einen Eintrag; true, wenn einer vorhanden war.
    pub fn remove(&self, ip: Ipv4Address) -> bool {
        self.entries.lock().remove(&ip).is_some()
    }

    /// Entfernt abgelaufene Einträge (ausser permanente).
    pub fn purge_expired(&self, now: u64) -> usize {
        let mut entries = self.entries.lock();
        let timeout = self.timeout_ns;
        let expired: Vec<Ipv4Address> = entries
            .iter()
            .filter(|(_, e)| !e.permanent && now > e.timestamp && now - e.timestamp > timeout)
            .map(|(&ip, _)| ip)
            .collect();
        for ip in &expired {
            entries.remove(ip);
        }
        expired.len()
    }

    /// Anzahl der Einträge, dauerhafte eingeschlossen.
    pub fn entry_count(&self) -> usize {
        self.entries.lock().len()
    }

    /// Entfernt alle Einträge, auch dauerhafte.
    pub fn clear(&self) {
        self.entries.lock().clear();
    }
}

// ─── ARP-Packet ────────────────────────────────────────────────────────────

/// Hardwaretyp Ethernet im ARP-Kopf.
pub const ARP_HW_ETHERNET: u16 = 1;
/// Operationscode einer Anfrage.
pub const ARP_OP_REQUEST: u16 = 1;
/// Operationscode einer Antwort.
pub const ARP_OP_REPLY: u16 = 2;

/// ARP-Paket zur Auflösung einer IPv4-Adresse in eine MAC-Adresse.
#[derive(Clone, Debug, PartialEq)]
pub struct ArpPacket {
    /// Hardwaretyp; für Ethernet 1.
    pub hw_type: u16,
    /// Protokolltyp der aufzulösenden Adresse.
    pub proto_type: u16,
    /// Länge der Hardwareadresse in Byte; für Ethernet 6.
    pub hw_size: u8,
    /// Länge der Protokolladresse in Byte; für IPv4 4.
    pub proto_size: u8,
    /// Anfrage oder Antwort.
    pub opcode: u16,
    /// MAC-Adresse des Absenders.
    pub sender_mac: MacAddress,
    /// IP-Adresse des Absenders.
    pub sender_ip: Ipv4Address,
    /// MAC-Adresse des Ziels; bei einer Anfrage die Nulladresse.
    pub target_mac: MacAddress,
    /// IP-Adresse des Ziels.
    pub target_ip: Ipv4Address,
}

impl ArpPacket {
    /// Anfrage für eine Ziel-IP; die Ziel-MAC bleibt Null.
    pub fn request(sender_mac: MacAddress, sender_ip: Ipv4Address, target_ip: Ipv4Address) -> Self {
        ArpPacket {
            hw_type: ARP_HW_ETHERNET,
            proto_type: ETH_TYPE_IPV4,
            hw_size: 6,
            proto_size: 4,
            opcode: ARP_OP_REQUEST,
            sender_mac,
            sender_ip,
            target_mac: MacAddress::zero(),
            target_ip,
        }
    }

    /// Antwort mit beiden Adresspaaren.
    pub fn reply(
        sender_mac: MacAddress,
        sender_ip: Ipv4Address,
        target_mac: MacAddress,
        target_ip: Ipv4Address,
    ) -> Self {
        ArpPacket {
            hw_type: ARP_HW_ETHERNET,
            proto_type: ETH_TYPE_IPV4,
            hw_size: 6,
            proto_size: 4,
            opcode: ARP_OP_REPLY,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        }
    }

    /// Paket als 28 Byte in Netzwerkreihenfolge.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(28);
        buf.extend_from_slice(&self.hw_type.to_be_bytes());
        buf.extend_from_slice(&self.proto_type.to_be_bytes());
        buf.push(self.hw_size);
        buf.push(self.proto_size);
        buf.extend_from_slice(&self.opcode.to_be_bytes());
        buf.extend_from_slice(&self.sender_mac.0);
        buf.extend_from_slice(&self.sender_ip.0);
        buf.extend_from_slice(&self.target_mac.0);
        buf.extend_from_slice(&self.target_ip.0);
        buf
    }

    /// Liest ein Paket aus mindestens 28 Byte; kürzere Eingaben ergeben PacketTooShort.
    pub fn from_bytes(data: &[u8]) -> Result<Self, NetworkError> {
        if data.len() < 28 {
            return Err(NetworkError::PacketTooShort);
        }
        let hw_type = u16::from_be_bytes([data[0], data[1]]);
        let proto_type = u16::from_be_bytes([data[2], data[3]]);
        let hw_size = data[4];
        let proto_size = data[5];
        let opcode = u16::from_be_bytes([data[6], data[7]]);

        let mut sender_mac = [0u8; 6];
        sender_mac.copy_from_slice(&data[8..14]);
        let mut sender_ip = [0u8; 4];
        sender_ip.copy_from_slice(&data[14..18]);
        let mut target_mac = [0u8; 6];
        target_mac.copy_from_slice(&data[18..24]);
        let mut target_ip = [0u8; 4];
        target_ip.copy_from_slice(&data[24..28]);

        Ok(ArpPacket {
            hw_type,
            proto_type,
            hw_size,
            proto_size,
            opcode,
            sender_mac: MacAddress(sender_mac),
            sender_ip: Ipv4Address(sender_ip),
            target_mac: MacAddress(target_mac),
            target_ip: Ipv4Address(target_ip),
        })
    }
}

// ─── NetworkDevice Trait ────────────────────────────────────────────────────

/// Vertrag eines Netzgeräts: Rahmen senden und empfangen, dazu MAC, MTU und Zustand.
pub trait NetworkDevice: Send + Sync {
    /// Sendet ein Ethernet-Frame.
    fn send_frame(&self, frame: &[u8]) -> Result<(), NetworkError>;

    /// Empfängt ein Frame (blockierend oder nicht, je nach Implementierung).
    fn recv_frame(&self) -> Result<Vec<u8>, NetworkError>;

    /// MAC-Adresse des Geräts.
    fn mac_address(&self) -> MacAddress;

    /// MTU (Maximum Transmission Unit) in Bytes.
    fn mtu(&self) -> usize {
        1500
    }

    /// Ob das Gerät "up" ist (verlinkt).
    fn is_up(&self) -> bool {
        true
    }

    /// Gerätename.
    fn name(&self) -> &str {
        "net-device"
    }
}

// ─── LoopbackDevice (für Tests) ─────────────────────────────────────────────

/// Gerät, das gesendete Rahmen in die eigene Empfangswarteschlange legt.
pub struct LoopbackDevice {
    mac: MacAddress,
    queue: Mutex<Vec<Vec<u8>>>,
    dev_name: String,
}

impl LoopbackDevice {
    /// Gerät mit der festen MAC 02:00:00:00:00:01 und leerer Warteschlange.
    pub fn new(name: &str) -> Self {
        LoopbackDevice {
            mac: MacAddress::new(0x02, 0x00, 0x00, 0x00, 0x00, 0x01),
            queue: Mutex::new(Vec::new()),
            dev_name: name.to_string(),
        }
    }

    /// Anzahl der wartenden Rahmen.
    pub fn queue_len(&self) -> usize {
        self.queue.lock().len()
    }
}

impl NetworkDevice for LoopbackDevice {
    fn send_frame(&self, frame: &[u8]) -> Result<(), NetworkError> {
        // Loopback: Frame wird in die Empfangs-Queue gesteckt
        self.queue.lock().push(frame.to_vec());
        Ok(())
    }

    fn recv_frame(&self) -> Result<Vec<u8>, NetworkError> {
        let mut queue = self.queue.lock();
        queue.pop().ok_or(NetworkError::NoFrameAvailable)
    }

    fn mac_address(&self) -> MacAddress {
        self.mac
    }
    fn name(&self) -> &str {
        &self.dev_name
    }
}

// ─── NetworkError ──────────────────────────────────────────────────────────

/// Fehler der Netzschicht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    /// Die Bytefolge ist kürzer als ein Ethernet-Rahmen.
    FrameTooShort,
    /// Die Bytefolge ist kürzer als das erwartete Paket.
    PacketTooShort,
    /// Die Prüfsumme stimmt nicht.
    InvalidChecksum,
    /// Es liegt kein Rahmen zum Abholen bereit.
    NoFrameAvailable,
    /// Das Gerät ist nicht verbunden.
    DeviceDown,
    /// Das Senden ist gescheitert; der Text nennt den Grund.
    SendFailed(String),
    /// Das Empfangen ist gescheitert; der Text nennt den Grund.
    RecvFailed(String),
    /// Die Adresse ließ sich nicht auflösen.
    ArpResolutionFailed,
    /// Das Protokoll wird nicht unterstützt.
    UnsupportedProtocol,
}

// ─── NetworkStack (Höchste Ebene — verbindet Device + ARP) ──────────────────

/// Verbindet ein Gerät mit der ARP-Tabelle und der eigenen Adresse.
pub struct NetworkStack {
    /// Gerät, über das gesendet und empfangen wird.
    pub device: Arc<dyn NetworkDevice>,
    arp: ArpTable,
    our_ip: Ipv4Address,
}

impl NetworkStack {
    /// Stack über einem Gerät, mit 30 Sekunden Ablaufzeit für ARP-Einträge.
    pub fn new(device: Arc<dyn NetworkDevice>, our_ip: Ipv4Address) -> Self {
        NetworkStack {
            device,
            arp: ArpTable::new(30_000_000_000), // 30s ARP timeout
            our_ip,
        }
    }

    /// Sendet ein ARP-Request für eine IP-Adresse.
    pub fn arp_request(&self, target_ip: Ipv4Address) -> Result<(), NetworkError> {
        let arp = ArpPacket::request(self.device.mac_address(), self.our_ip, target_ip);
        let frame = EthernetFrame::new(
            MacAddress::broadcast(),
            self.device.mac_address(),
            ETH_TYPE_ARP,
            arp.to_bytes(),
        );
        self.device.send_frame(&frame.to_bytes())
    }

    /// Verarbeitet ein empfangenes Frame.
    pub fn handle_frame(&self, data: &[u8], timestamp: u64) -> Result<(), NetworkError> {
        let frame = EthernetFrame::from_bytes(data)?;

        match frame.ethertype {
            ETH_TYPE_ARP => self.handle_arp(&frame.payload, timestamp),
            ETH_TYPE_IPV4 => Ok(()), // IPv4 handling would go here
            _ => Err(NetworkError::UnsupportedProtocol),
        }
    }

    /// Verarbeitet ein ARP-Packet.
    fn handle_arp(&self, payload: &[u8], timestamp: u64) -> Result<(), NetworkError> {
        let arp = ArpPacket::from_bytes(payload)?;

        // Lerne Sender-MAC
        self.arp.insert(arp.sender_ip, arp.sender_mac, timestamp);

        // Wenn ARP-Request an uns: antworten
        if arp.opcode == ARP_OP_REQUEST && arp.target_ip == self.our_ip {
            let reply = ArpPacket::reply(
                self.device.mac_address(),
                self.our_ip,
                arp.sender_mac,
                arp.sender_ip,
            );
            let frame = EthernetFrame::new(
                arp.sender_mac,
                self.device.mac_address(),
                ETH_TYPE_ARP,
                reply.to_bytes(),
            );
            self.device.send_frame(&frame.to_bytes())?;
        }

        Ok(())
    }

    /// Löst eine IP-Adresse zu einer MAC auf (via ARP-Cache).
    pub fn resolve_mac(&self, ip: Ipv4Address) -> Option<MacAddress> {
        self.arp.lookup(ip)
    }

    /// ARP-Cache-Eintraganzahl.
    pub fn arp_entries(&self) -> usize {
        self.arp.entry_count()
    }

    /// Unsere IP-Adresse.
    pub fn our_ip(&self) -> Ipv4Address {
        self.our_ip
    }

    /// Sendet ein Frame an eine bekannte MAC.
    pub fn send_to(
        &self,
        dst_mac: MacAddress,
        ethertype: u16,
        payload: Vec<u8>,
    ) -> Result<(), NetworkError> {
        let frame = EthernetFrame::new(dst_mac, self.device.mac_address(), ethertype, payload);
        self.device.send_frame(&frame.to_bytes())
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── MAC-Adresse ─────────────────────────────────────────────────────────

    #[test]
    fn test_mac_create() {
        let mac = MacAddress::new(0xDE, 0xAD, 0xBE, 0xEF, 0x42, 0x00);
        assert_eq!(mac.0, [0xDE, 0xAD, 0xBE, 0xEF, 0x42, 0x00]);
    }

    #[test]
    fn test_mac_broadcast() {
        let mac = MacAddress::broadcast();
        assert!(mac.is_broadcast());
        assert!(!mac.is_zero());
    }

    #[test]
    fn test_mac_zero() {
        let mac = MacAddress::zero();
        assert!(mac.is_zero());
        assert!(!mac.is_broadcast());
    }

    #[test]
    fn test_mac_to_string() {
        let mac = MacAddress::new(0xDE, 0xAD, 0xBE, 0xEF, 0x42, 0x00);
        assert_eq!(mac.to_string(), "de:ad:be:ef:42:00");
    }

    // ── IPv4 ────────────────────────────────────────────────────────────────

    #[test]
    fn test_ipv4_create() {
        let ip = Ipv4Address::new(192, 168, 1, 1);
        assert_eq!(ip.0, [192, 168, 1, 1]);
    }

    #[test]
    fn test_ipv4_broadcast() {
        let ip = Ipv4Address::broadcast();
        assert!(ip.is_broadcast());
    }

    #[test]
    fn test_ipv4_to_string() {
        let ip = Ipv4Address::new(10, 0, 0, 1);
        assert_eq!(ip.to_string(), "10.0.0.1");
    }

    // ── Ethernet-Frame ─────────────────────────────────────────────────────

    #[test]
    fn test_ethernet_frame_serialize() {
        let frame = EthernetFrame::new(
            MacAddress::new(0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF),
            MacAddress::new(0xDE, 0xAD, 0xBE, 0xEF, 0x42, 0x00),
            ETH_TYPE_ARP,
            vec![0x01, 0x02, 0x03],
        );
        let bytes = frame.to_bytes();
        assert_eq!(bytes.len(), 14 + 3);
        assert_eq!(&bytes[0..6], &[0xFF; 6]); // dst
        assert_eq!(&bytes[6..12], &[0xDE, 0xAD, 0xBE, 0xEF, 0x42, 0x00]); // src
        assert_eq!(&bytes[12..14], &[0x08, 0x06]); // ethertype ARP (big-endian)
    }

    #[test]
    fn test_ethernet_frame_deserialize() {
        let frame = EthernetFrame::new(
            MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
            MacAddress::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            ETH_TYPE_IPV4,
            vec![0xDE, 0xAD],
        );
        let bytes = frame.to_bytes();
        let parsed = EthernetFrame::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.dst_mac, frame.dst_mac);
        assert_eq!(parsed.src_mac, frame.src_mac);
        assert_eq!(parsed.ethertype, ETH_TYPE_IPV4);
        assert_eq!(parsed.payload, frame.payload);
    }

    #[test]
    fn test_ethernet_frame_too_short() {
        let data = [0u8; 10]; // < 14
        assert_eq!(
            EthernetFrame::from_bytes(&data),
            Err(NetworkError::FrameTooShort)
        );
    }

    // ── ARP-Packet ──────────────────────────────────────────────────────────

    #[test]
    fn test_arp_request_create() {
        let arp = ArpPacket::request(
            MacAddress::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            Ipv4Address::new(192, 168, 1, 1),
            Ipv4Address::new(192, 168, 1, 2),
        );
        assert_eq!(arp.opcode, ARP_OP_REQUEST);
        assert_eq!(arp.hw_type, ARP_HW_ETHERNET);
        assert_eq!(arp.proto_type, ETH_TYPE_IPV4);
        assert!(arp.target_mac.is_zero());
    }

    #[test]
    fn test_arp_reply_create() {
        let arp = ArpPacket::reply(
            MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
            Ipv4Address::new(10, 0, 0, 1),
            MacAddress::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            Ipv4Address::new(10, 0, 0, 2),
        );
        assert_eq!(arp.opcode, ARP_OP_REPLY);
        assert_eq!(arp.target_mac.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn test_arp_serialize_deserialize() {
        let arp = ArpPacket::request(
            MacAddress::new(0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01),
            Ipv4Address::new(192, 168, 1, 100),
            Ipv4Address::new(192, 168, 1, 200),
        );
        let bytes = arp.to_bytes();
        assert_eq!(bytes.len(), 28);

        let parsed = ArpPacket::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.opcode, ARP_OP_REQUEST);
        assert_eq!(parsed.sender_mac, arp.sender_mac);
        assert_eq!(parsed.sender_ip, arp.sender_ip);
        assert_eq!(parsed.target_ip, arp.target_ip);
    }

    #[test]
    fn test_arp_packet_too_short() {
        let data = [0u8; 10];
        assert_eq!(
            ArpPacket::from_bytes(&data),
            Err(NetworkError::PacketTooShort)
        );
    }

    // ── ARP-Table ────────────────────────────────────────────────────────────

    #[test]
    fn test_arp_table_insert_lookup() {
        let table = ArpTable::new(30_000_000_000);
        let ip = Ipv4Address::new(192, 168, 1, 10);
        let mac = MacAddress::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF);

        table.insert(ip, mac, 0);
        assert_eq!(table.entry_count(), 1);
        assert_eq!(table.lookup(ip), Some(mac));
    }

    #[test]
    fn test_arp_table_permanent() {
        let table = ArpTable::new(1_000_000_000);
        let ip = Ipv4Address::new(10, 0, 0, 1);
        let mac = MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66);

        table.insert_permanent(ip, mac);
        assert_eq!(table.lookup(ip), Some(mac));

        // Permanent entries sollten nicht expirieren
        table.purge_expired(999_999_999_999);
        assert_eq!(table.entry_count(), 1);
    }

    #[test]
    fn test_arp_table_remove() {
        let table = ArpTable::new(30_000_000_000);
        let ip = Ipv4Address::new(172, 16, 0, 1);
        table.insert(ip, MacAddress::new(1, 2, 3, 4, 5, 6), 0);
        assert!(table.remove(ip));
        assert_eq!(table.entry_count(), 0);
        assert!(table.lookup(ip).is_none());
    }

    #[test]
    fn test_arp_table_purge_expired() {
        let table = ArpTable::new(1_000_000_000); // 1s timeout
        table.insert(
            Ipv4Address::new(10, 0, 0, 1),
            MacAddress::new(1, 2, 3, 4, 5, 6),
            0,
        );
        table.insert(
            Ipv4Address::new(10, 0, 0, 2),
            MacAddress::new(7, 8, 9, 10, 11, 12),
            0,
        );

        // Nach 2s: beide abgelaufen
        let purged = table.purge_expired(2_000_000_000);
        assert_eq!(purged, 2);
        assert_eq!(table.entry_count(), 0);
    }

    #[test]
    fn test_arp_table_not_expired_yet() {
        let table = ArpTable::new(10_000_000_000); // 10s timeout
        table.insert(
            Ipv4Address::new(10, 0, 0, 1),
            MacAddress::new(1, 2, 3, 4, 5, 6),
            0,
        );
        let purged = table.purge_expired(5_000_000_000); // 5s
        assert_eq!(purged, 0);
        assert_eq!(table.entry_count(), 1);
    }

    // ── LoopbackDevice ──────────────────────────────────────────────────────

    #[test]
    fn test_loopback_send_recv() {
        let dev = LoopbackDevice::new("lo0");
        let frame = EthernetFrame::new(
            MacAddress::broadcast(),
            dev.mac_address(),
            ETH_TYPE_ARP,
            vec![0xDE, 0xAD],
        );
        let bytes = frame.to_bytes();

        dev.send_frame(&bytes).unwrap();
        assert_eq!(dev.queue_len(), 1);

        let recv = dev.recv_frame().unwrap();
        assert_eq!(recv, bytes);
        assert_eq!(dev.queue_len(), 0);
    }

    #[test]
    fn test_loopback_recv_empty() {
        let dev = LoopbackDevice::new("lo0");
        assert_eq!(dev.recv_frame(), Err(NetworkError::NoFrameAvailable));
    }

    // ── NetworkStack: ARP-Request/Reply ─────────────────────────────────────

    #[test]
    fn test_network_stack_arp_request() {
        let dev = Arc::new(LoopbackDevice::new("lo0"));
        let stack = NetworkStack::new(dev.clone(), Ipv4Address::new(10, 0, 0, 1));

        stack.arp_request(Ipv4Address::new(10, 0, 0, 2)).unwrap();

        // Loopback hat das Frame in der Queue
        assert_eq!(dev.queue_len(), 1);

        // Frame empfangen und verarbeiten (als ob es vom 10.0.0.2 käme)
        let frame_data = dev.recv_frame().unwrap();
        let frame = EthernetFrame::from_bytes(&frame_data).unwrap();
        assert_eq!(frame.ethertype, ETH_TYPE_ARP);
        assert!(frame.dst_mac.is_broadcast());
    }

    #[test]
    fn test_network_stack_arp_reply_learning() {
        let dev = Arc::new(LoopbackDevice::new("lo0"));
        let stack = NetworkStack::new(dev.clone(), Ipv4Address::new(10, 0, 0, 1));

        // Simuliere: ein anderer Node sendet ARP-Reply an uns
        let reply = ArpPacket::reply(
            MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
            Ipv4Address::new(10, 0, 0, 2),
            stack.device.mac_address(),
            Ipv4Address::new(10, 0, 0, 1),
        );
        let frame = EthernetFrame::new(
            stack.device.mac_address(),
            MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
            ETH_TYPE_ARP,
            reply.to_bytes(),
        );

        // Verarbeite das Frame
        stack.handle_frame(&frame.to_bytes(), 0).unwrap();

        // ARP-Cache sollte jetzt 10.0.0.2 kennen
        assert_eq!(stack.arp_entries(), 1);
        assert_eq!(
            stack.resolve_mac(Ipv4Address::new(10, 0, 0, 2)),
            Some(MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66))
        );
    }

    #[test]
    fn test_network_stack_arp_request_to_us_triggers_reply() {
        let dev = Arc::new(LoopbackDevice::new("lo0"));
        let our_ip = Ipv4Address::new(10, 0, 0, 1);
        let stack = NetworkStack::new(dev.clone(), our_ip);

        // Simuliere ARP-Request von 10.0.0.2 an uns
        let request = ArpPacket::request(
            MacAddress::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            Ipv4Address::new(10, 0, 0, 2),
            our_ip,
        );
        let frame = EthernetFrame::new(
            stack.device.mac_address(),
            MacAddress::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
            ETH_TYPE_ARP,
            request.to_bytes(),
        );

        stack.handle_frame(&frame.to_bytes(), 0).unwrap();

        // Der Stack sollte einen ARP-Reply gesendet haben (via Loopback in Queue)
        assert_eq!(dev.queue_len(), 1);
        let reply_data = dev.recv_frame().unwrap();
        let reply_frame = EthernetFrame::from_bytes(&reply_data).unwrap();
        let reply_arp = ArpPacket::from_bytes(&reply_frame.payload).unwrap();

        assert_eq!(reply_arp.opcode, ARP_OP_REPLY);
        assert_eq!(reply_arp.sender_ip, our_ip);
        assert_eq!(reply_arp.target_ip, Ipv4Address::new(10, 0, 0, 2));
    }

    #[test]
    fn test_network_stack_send_to_known_mac() {
        let dev = Arc::new(LoopbackDevice::new("lo0"));
        let stack = NetworkStack::new(dev.clone(), Ipv4Address::new(10, 0, 0, 1));

        let dst = MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66);
        stack
            .send_to(dst, ETH_TYPE_IPV4, vec![0x01, 0x02, 0x03])
            .unwrap();

        assert_eq!(dev.queue_len(), 1);
        let data = dev.recv_frame().unwrap();
        let frame = EthernetFrame::from_bytes(&data).unwrap();
        assert_eq!(frame.dst_mac, dst);
        assert_eq!(frame.ethertype, ETH_TYPE_IPV4);
    }

    #[test]
    fn test_network_stack_unsupported_protocol() {
        let dev = Arc::new(LoopbackDevice::new("lo0"));
        let stack = NetworkStack::new(dev, Ipv4Address::new(10, 0, 0, 1));

        let frame = EthernetFrame::new(
            MacAddress::broadcast(),
            MacAddress::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
            0x1234, // unsupported
            vec![0x00],
        );

        let result = stack.handle_frame(&frame.to_bytes(), 0);
        assert_eq!(result, Err(NetworkError::UnsupportedProtocol));
    }
}
