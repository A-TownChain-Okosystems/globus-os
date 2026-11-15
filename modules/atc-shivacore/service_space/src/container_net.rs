// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ══════════════════════════════════════════════════════════════════════════════
// K-Sprint 47 — Container Networking
// Network Namespaces, veth Pairs, Bridge, IP Allocation,
// Port Forwarding, Firewall Rules (nftables-style), DNS per Namespace.
// ══════════════════════════════════════════════════════════════════════════════

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::format;
use alloc::string::{String, ToString};
#[allow(unused_imports)]
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

static VETH_SEQ: AtomicU64 = AtomicU64::new(0);
static NS_SEQ: AtomicU64 = AtomicU64::new(0);
static RULE_SEQ: AtomicU64 = AtomicU64::new(0);
static PORTFWD_SEQ: AtomicU64 = AtomicU64::new(0);
static DNS_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_veth_id() -> u64 {
    VETH_SEQ.fetch_add(1, Ordering::SeqCst)
}
fn next_ns_id() -> u64 {
    NS_SEQ.fetch_add(1, Ordering::SeqCst)
}
fn next_rule_id() -> u64 {
    RULE_SEQ.fetch_add(1, Ordering::SeqCst)
}
fn next_portfwd_id() -> u64 {
    PORTFWD_SEQ.fetch_add(1, Ordering::SeqCst)
}
fn next_dns_id() -> u64 {
    DNS_SEQ.fetch_add(1, Ordering::SeqCst)
}

// ══════════════════════════════════════════════════════════════════════════════
// IP ADDRESSES
// ══════════════════════════════════════════════════════════════════════════════

/// IPv4-Adresse in Netzwerkbytefolge.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    /// Adresse aus ihren vier Oktetten.
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }
    /// Unbestimmte Adresse 0.0.0.0.
    pub fn zero() -> Self {
        Self([0, 0, 0, 0])
    }
    /// True fuer 0.0.0.0.
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }
    /// Adresse als 32-Bit-Zahl in Hostbytefolge.
    pub fn to_u32(&self) -> u32 {
        ((self.0[0] as u32) << 24)
            | ((self.0[1] as u32) << 16)
            | ((self.0[2] as u32) << 8)
            | (self.0[3] as u32)
    }
    /// Adresse aus einer 32-Bit-Zahl.
    pub fn from_u32(v: u32) -> Self {
        Self([(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8])
    }
    /// Adresse in Punktnotation, etwa `10.0.0.2`.
    pub fn as_dotted(&self) -> String {
        format!("{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
    /// True, wenn die Adresse im angegebenen Subnetz liegt.
    pub fn in_subnet(&self, net: &IpSubnet) -> bool {
        let addr = self.to_u32();
        let net_addr = net.network_addr.to_u32();
        let mask = net.netmask();
        (addr & mask) == (net_addr & mask)
    }
}

impl core::fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.as_dotted())
    }
}

/// MAC-Adresse in Netzwerkbytefolge.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct MacAddr(pub [u8; 6]);

impl MacAddr {
    /// Adresse aus ihren sechs Oktetten.
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        Self([a, b, c, d, e, f])
    }
    /// Broadcast-Adresse ff:ff:ff:ff:ff:ff.
    pub fn broadcast() -> Self {
        Self([0xFF; 6])
    }
    /// Unbestimmte Adresse 00:00:00:00:00:00.
    pub fn zero() -> Self {
        Self([0; 6])
    }
    /// True fuer die Broadcast-Adresse.
    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 6]
    }
    /// True fuer die unbestimmte Adresse.
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 6]
    }
    /// Adresse in Doppelpunktnotation.
    pub fn as_colon(&self) -> String {
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
    /// Zufaellige Adresse mit lokal verwaltetem Bit.
    pub fn random() -> Self {
        let id = VETH_SEQ.fetch_add(1, Ordering::SeqCst);
        Self([
            0x02,
            0x42,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
            id as u8,
        ])
    }
}

impl core::fmt::Display for MacAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.as_colon())
    }
}

/// IPv4-Subnetz aus Netzadresse und Praefixlaenge.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct IpSubnet {
    /// Netzadresse des Subnetzes.
    pub network_addr: Ipv4Addr,
    /// Praefixlaenge in Bits, 0 bis 32.
    pub prefix_len: u8,
}

impl IpSubnet {
    /// Subnetz aus Adresse und Praefix.
    pub fn new(addr: Ipv4Addr, prefix: u8) -> Self {
        let mask = Self::mask_from_prefix(prefix);
        let net = Ipv4Addr::from_u32(addr.to_u32() & mask);
        Self {
            network_addr: net,
            prefix_len: prefix,
        }
    }
    /// Netzmaske als 32-Bit-Zahl.
    pub fn netmask(&self) -> u32 {
        Self::mask_from_prefix(self.prefix_len)
    }
    /// Netzmaske zu einer Praefixlaenge; ungueltige Laengen werden auf den Rand gezogen.
    pub fn mask_from_prefix(prefix: u8) -> u32 {
        if prefix == 0 {
            return 0;
        }
        if prefix >= 32 {
            return 0xFFFF_FFFF;
        }
        !0u32 << (32 - prefix)
    }
    /// Broadcast-Adresse des Subnetzes.
    pub fn broadcast_addr(&self) -> Ipv4Addr {
        Ipv4Addr::from_u32(self.network_addr.to_u32() | !self.netmask())
    }
    /// Nutzbare Hostadressen im Subnetz.
    pub fn host_capacity(&self) -> u64 {
        if self.prefix_len >= 32 {
            1
        } else if self.prefix_len >= 31 {
            2
        } else {
            (1u64 << (32 - self.prefix_len)) - 2
        }
    }
    /// True, wenn die Adresse im Subnetz liegt.
    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        addr.in_subnet(self)
    }
}

impl core::fmt::Display for IpSubnet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}/{}", self.network_addr, self.prefix_len)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// NETWORK NAMESPACE
// ══════════════════════════════════════════════════════════════════════════════

/// Kennung eines Netzwerk-Namespace.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct NetNsId(pub u64);

/// Ein Netzwerk-Namespace mit eigenen Schnittstellen, Routen und Namensaufloesung.
#[derive(Clone, Debug)]
pub struct NetworkNamespace {
    /// Kennung des Namespace.
    pub id: NetNsId,
    /// Name des Namespace.
    pub name: String,
    /// PID des Containers, der den Namespace haelt.
    pub container_pid: u32,
    /// Schnittstellen des Namespace, nach ID sortiert.
    pub interfaces: BTreeMap<u64, NetInterface>,
    /// Routentabelle des Namespace.
    pub routing_table: Vec<Route>,
    /// Namensaufloesung des Namespace.
    pub dns_config: DnsConfig,
    /// True, wenn das Loopback-Interface oben ist.
    pub loopback_up: bool,
    /// Zeitpunkt der Erstellung in Timer-Ticks.
    pub created_at: u64,
    /// Zaehler des Namespace.
    pub stats: NetNsStats,
}

impl NetworkNamespace {
    /// Legt einen Namespace mit oben liegendem Loopback an.
    pub fn new(name: &str, container_pid: u32) -> Self {
        let id = NetNsId(next_ns_id());
        let mut interfaces = BTreeMap::new();
        let lo = NetInterface {
            id: 0,
            name: "lo".to_string(),
            kind: InterfaceKind::Loopback,
            mac: MacAddr::zero(),
            ip: Ipv4Addr::new(127, 0, 0, 1),
            prefix_len: 8,
            mtu: 65536,
            up: true,
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            rx_errors: 0,
            tx_errors: 0,
            peer_ns: None,
            peer_iface: None,
        };
        interfaces.insert(0, lo);
        Self {
            id,
            name: name.to_string(),
            container_pid,
            interfaces,
            routing_table: vec![Route {
                dest: IpSubnet::new(Ipv4Addr::new(127, 0, 0, 1), 8),
                gateway: Ipv4Addr::zero(),
                iface_id: 0,
                metric: 0,
            }],
            dns_config: DnsConfig::default(),
            loopback_up: true,
            created_at: 0,
            stats: NetNsStats::default(),
        }
    }
    /// Fuegt eine Schnittstelle hinzu und liefert ihre ID.
    pub fn add_interface(&mut self, iface: NetInterface) -> u64 {
        let id = iface.id;
        self.interfaces.insert(id, iface);
        id
    }
    /// Entfernt eine Schnittstelle und liefert sie zurueck.
    pub fn remove_interface(&mut self, iface_id: u64) -> Option<NetInterface> {
        self.interfaces.remove(&iface_id)
    }
    /// Schnittstelle nach ID.
    pub fn get_interface(&self, id: u64) -> Option<&NetInterface> {
        self.interfaces.get(&id)
    }
    /// Schnittstelle nach ID, veraenderbar.
    pub fn get_interface_mut(&mut self, id: u64) -> Option<&mut NetInterface> {
        self.interfaces.get_mut(&id)
    }
    /// Alle Schnittstellen des Namespace.
    pub fn list_interfaces(&self) -> Vec<&NetInterface> {
        self.interfaces.values().collect()
    }
    /// Traegt eine Route ein.
    pub fn add_route(&mut self, route: Route) {
        self.routing_table.push(route);
        self.routing_table.sort_by_key(|r| r.metric);
    }
    /// Entfernt die Route zu einem Zielnetz.
    pub fn remove_route(&mut self, dest: IpSubnet) -> bool {
        let b = self.routing_table.len();
        self.routing_table.retain(|r| r.dest != dest);
        self.routing_table.len() < b
    }
    /// Passende Route fuer eine Zieladresse; die spezifischste gewinnt.
    pub fn lookup_route(&self, dest: Ipv4Addr) -> Option<&Route> {
        let mut best: Option<&Route> = None;
        let mut best_p = 0u8;
        for r in &self.routing_table {
            if r.dest.contains(dest) && r.dest.prefix_len >= best_p {
                best = Some(r);
                best_p = r.dest.prefix_len;
            }
        }
        best
    }
    /// Setzt Namensserver und Suchdomains.
    pub fn set_dns(&mut self, servers: Vec<Ipv4Addr>, search: Vec<String>) {
        self.dns_config = DnsConfig {
            servers,
            search_domains: search,
            ..Default::default()
        };
    }
    /// Setzt eine Schnittstelle auf oben oder unten.
    pub fn set_interface_up(&mut self, id: u64, up: bool) -> bool {
        if let Some(i) = self.interfaces.get_mut(&id) {
            i.up = up;
            if i.kind == InterfaceKind::Loopback {
                self.loopback_up = up;
            }
            true
        } else {
            false
        }
    }
    /// Setzt Adresse und Praefix einer Schnittstelle.
    pub fn set_interface_ip(&mut self, id: u64, ip: Ipv4Addr, p: u8) -> bool {
        if let Some(i) = self.interfaces.get_mut(&id) {
            i.ip = ip;
            i.prefix_len = p;
            true
        } else {
            false
        }
    }
    /// Setzt die MTU einer Schnittstelle.
    pub fn set_mtu(&mut self, id: u64, mtu: u32) -> bool {
        if !(68..=65536).contains(&mtu) {
            return false;
        }
        if let Some(i) = self.interfaces.get_mut(&id) {
            i.mtu = mtu;
            true
        } else {
            false
        }
    }
    /// Zaehlt empfangene Bytes und Pakete einer Schnittstelle.
    pub fn rx(&mut self, id: u64, b: u64, p: u64) {
        if let Some(i) = self.interfaces.get_mut(&id) {
            i.rx_bytes += b;
            i.rx_packets += p;
            self.stats.total_rx_bytes += b;
            self.stats.total_rx_packets += p;
        }
    }
    /// Zaehlt gesendete Bytes und Pakete einer Schnittstelle.
    pub fn tx(&mut self, id: u64, b: u64, p: u64) {
        if let Some(i) = self.interfaces.get_mut(&id) {
            i.tx_bytes += b;
            i.tx_packets += p;
            self.stats.total_tx_bytes += b;
            self.stats.total_tx_packets += p;
        }
    }
    /// Zaehlt einen Fehler in Empfangs- oder Senderichtung.
    pub fn record_error(&mut self, id: u64, is_rx: bool) {
        if let Some(i) = self.interfaces.get_mut(&id) {
            if is_rx {
                i.rx_errors += 1;
                self.stats.total_rx_errors += 1;
            } else {
                i.tx_errors += 1;
                self.stats.total_tx_errors += 1;
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// NETWORK INTERFACE
// ══════════════════════════════════════════════════════════════════════════════

/// Art einer Netzwerkschnittstelle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum InterfaceKind {
    /// Loopback-Schnittstelle.
    Loopback,
    /// Ende eines veth-Paares.
    Veth,
    /// Bridge-Schnittstelle.
    Bridge,
    /// Physische Schnittstelle.
    Physical,
    /// VLAN-Schnittstelle.
    Vlan,
    /// Gebundelte Schnittstelle.
    Bond,
}

impl InterfaceKind {
    /// Kurzname der Art.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Veth => "veth",
            Self::Bridge => "bridge",
            Self::Physical => "physical",
            Self::Vlan => "vlan",
            Self::Bond => "bond",
        }
    }
}

/// Eine Netzwerkschnittstelle mit Adresse, Zaehlern und Peer-Bezug.
#[derive(Clone, Debug)]
pub struct NetInterface {
    /// Kennung der Schnittstelle innerhalb ihres Namespace.
    pub id: u64,
    /// Schnittstellenname.
    pub name: String,
    /// Art der Schnittstelle.
    pub kind: InterfaceKind,
    /// MAC-Adresse.
    pub mac: MacAddr,
    /// IPv4-Adresse.
    pub ip: Ipv4Addr,
    /// Praefixlaenge der Adresse.
    pub prefix_len: u8,
    /// Maximale Nutzlast in Byte. u32, weil Loopback 65536 traegt und
    /// unzulaessige 70000 noch darstellbar sein muessen, um sie abzulehnen.
    pub mtu: u32,
    /// True, wenn die Schnittstelle oben ist.
    pub up: bool,
    /// Empfangene Bytes.
    pub rx_bytes: u64,
    /// Gesendete Bytes.
    pub tx_bytes: u64,
    /// Empfangene Pakete.
    pub rx_packets: u64,
    /// Gesendete Pakete.
    pub tx_packets: u64,
    /// Empfangsfehler.
    pub rx_errors: u64,
    /// Sendefehler.
    pub tx_errors: u64,
    /// Namespace des Gegenendes, falls vorhanden.
    pub peer_ns: Option<NetNsId>,
    /// ID des Gegenendes im Peer-Namespace.
    pub peer_iface: Option<u64>,
}

impl NetInterface {
    /// Legt ein veth-Ende ohne Adresse an.
    pub fn new_veth(name: &str) -> Self {
        Self {
            id: next_veth_id(),
            name: name.to_string(),
            kind: InterfaceKind::Veth,
            mac: MacAddr::random(),
            ip: Ipv4Addr::zero(),
            prefix_len: 32,
            mtu: 1500,
            up: false,
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            rx_errors: 0,
            tx_errors: 0,
            peer_ns: None,
            peer_iface: None,
        }
    }
    /// Legt eine Bridge-Schnittstelle an.
    pub fn new_bridge(name: &str) -> Self {
        Self {
            id: next_veth_id(),
            name: name.to_string(),
            kind: InterfaceKind::Bridge,
            mac: MacAddr::random(),
            ip: Ipv4Addr::zero(),
            prefix_len: 24,
            mtu: 1500,
            up: false,
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            rx_errors: 0,
            tx_errors: 0,
            peer_ns: None,
            peer_iface: None,
        }
    }
    /// Subnetz aus Adresse und Praefix der Schnittstelle.
    pub fn subnet(&self) -> IpSubnet {
        IpSubnet::new(self.ip, self.prefix_len)
    }
    /// True, wenn die Schnittstelle oben ist.
    pub fn is_up(&self) -> bool {
        self.up
    }
    /// True, wenn eine Adresse gesetzt ist.
    pub fn has_ip(&self) -> bool {
        !self.ip.is_zero()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ROUTING
// ══════════════════════════════════════════════════════════════════════════════

/// Ein Eintrag der Routentabelle.
#[derive(Clone, Debug)]
pub struct Route {
    /// Zielnetz.
    pub dest: IpSubnet,
    /// Naechster Hop; die Nulladresse bedeutet direkt verbunden.
    pub gateway: Ipv4Addr,
    /// Ausgehende Schnittstelle.
    pub iface_id: u64,
    /// Metrik; kleinere Werte werden bevorzugt.
    pub metric: u32,
}

impl Route {
    /// Standardroute ueber ein Gateway.
    pub fn default(gw: Ipv4Addr, iface: u64) -> Self {
        Self {
            dest: IpSubnet::new(Ipv4Addr::zero(), 0),
            gateway: gw,
            iface_id: iface,
            metric: 100,
        }
    }
    /// True fuer die Standardroute.
    pub fn is_default(&self) -> bool {
        self.dest.prefix_len == 0
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// DNS
// ══════════════════════════════════════════════════════════════════════════════

/// Namensaufloesung eines Namespace.
#[derive(Clone, Debug, Default)]
pub struct DnsConfig {
    /// Namensserver in Abfragereihenfolge.
    pub servers: Vec<Ipv4Addr>,
    /// Suchdomains fuer unvollstaendige Namen.
    pub search_domains: Vec<String>,
    /// Mindestzahl an Punkten, bevor ein Name als vollstaendig gilt.
    pub ndots: u8,
    /// Zeitlimit je Abfrage in Millisekunden.
    pub timeout_ms: u16,
    /// Wiederholungen je Abfrage.
    pub attempts: u8,
}

impl DnsConfig {
    /// Konfiguration mit Namensservern und Standardwerten.
    pub fn new(servers: Vec<Ipv4Addr>) -> Self {
        Self {
            servers,
            search_domains: vec![],
            ndots: 1,
            timeout_ms: 5000,
            attempts: 3,
        }
    }
}

/// Ein Eintrag der internen Namensaufloesung.
#[derive(Clone, Debug)]
pub struct DnsEntry {
    /// Kennung des Eintrags.
    pub id: u64,
    /// Hostname.
    pub hostname: String,
    /// Zugeordnete Adresse.
    pub ip: Ipv4Addr,
    /// Namespace, in dem der Name gilt.
    pub namespace: NetNsId,
    /// Gueltigkeit in Sekunden.
    pub ttl: u32,
    /// Zeitpunkt der Eintragung.
    pub created_at: u64,
}

// ══════════════════════════════════════════════════════════════════════════════
// VETH PAIR
// ══════════════════════════════════════════════════════════════════════════════

/// Ein veth-Paar zwischen Host und Container.
#[derive(Clone, Debug)]
pub struct VethPair {
    /// Kennung des Paares.
    pub id: u64,
    /// Hostseitige Schnittstelle.
    pub host_iface: NetInterface,
    /// Containerseitige Schnittstelle.
    pub container_iface: NetInterface,
    /// Namespace der Hostseite.
    pub host_ns: NetNsId,
    /// Namespace der Containerseite.
    pub container_ns: NetNsId,
    /// Bridge, an der das Hostende haengt.
    pub bridge_iface: Option<u64>,
    /// Zeitpunkt der Erstellung.
    pub created_at: u64,
}

impl VethPair {
    /// Legt beide Enden an und verknuepft sie miteinander.
    pub fn new(host_name: &str, c_name: &str, host_ns: NetNsId, c_ns: NetNsId) -> Self {
        let id = next_veth_id();
        let mut host = NetInterface::new_veth(host_name);
        let mut container = NetInterface::new_veth(c_name);
        host.peer_ns = Some(c_ns);
        host.peer_iface = Some(container.id);
        container.peer_ns = Some(host_ns);
        container.peer_iface = Some(host.id);
        Self {
            id,
            host_iface: host,
            container_iface: container,
            host_ns,
            container_ns: c_ns,
            bridge_iface: None,
            created_at: 0,
        }
    }
    /// Setzt Adresse und Praefix am Containerende.
    pub fn set_container_ip(&mut self, ip: Ipv4Addr, p: u8) {
        self.container_iface.ip = ip;
        self.container_iface.prefix_len = p;
    }
    /// Setzt Adresse und Praefix am Hostende.
    pub fn set_host_ip(&mut self, ip: Ipv4Addr, p: u8) {
        self.host_iface.ip = ip;
        self.host_iface.prefix_len = p;
    }
    /// Setzt beide Enden auf oben.
    pub fn bring_up(&mut self) {
        self.host_iface.up = true;
        self.container_iface.up = true;
    }
    /// Setzt beide Enden auf unten.
    pub fn tear_down(&mut self) {
        self.host_iface.up = false;
        self.container_iface.up = false;
    }
    /// Haengt das Hostende an eine Bridge.
    pub fn attach_to_bridge(&mut self, b: u64) {
        self.bridge_iface = Some(b);
    }
    /// Loest das Hostende von seiner Bridge.
    pub fn detach_from_bridge(&mut self) {
        self.bridge_iface = None;
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// BRIDGE
// ══════════════════════════════════════════════════════════════════════════════

/// Eine Bridge mit Ports, Lerntabelle und Zaehlern.
#[derive(Clone, Debug)]
pub struct Bridge {
    /// Kennung der Bridge.
    pub id: u64,
    /// Name der Bridge.
    pub name: String,
    /// Schnittstelle der Bridge.
    pub iface: NetInterface,
    /// Ports der Bridge.
    pub ports: BTreeSet<u64>,
    /// True, wenn Spanning Tree aktiv ist.
    pub stp_enabled: bool,
    /// Gewaehlte Wurzel fuer Spanning Tree.
    pub stp_root: Option<u64>,
    /// ARP-Tabelle der Bridge.
    pub arp_table: BTreeMap<Ipv4Addr, MacAddr>,
    /// Lerntabelle von MAC-Adresse zu Port.
    pub mac_table: BTreeMap<MacAddr, u64>,
    /// Subnetz der Bridge, falls gesetzt.
    pub subnet: Option<IpSubnet>,
    /// Gateway-Adresse der Bridge.
    pub gateway_ip: Option<Ipv4Addr>,
    /// DHCP-Bereich der Bridge, falls aktiviert.
    pub dhcp_pool: Option<DhcpPool>,
    /// Zeitpunkt der Erstellung.
    pub created_at: u64,
    /// Zaehler der Bridge.
    pub stats: BridgeStats,
}

/// Zaehler einer Bridge.
#[derive(Clone, Debug, Default)]
pub struct BridgeStats {
    /// Weitergeleitete Frames.
    pub frames_forwarded: u64,
    /// Verworfene Frames.
    pub frames_dropped: u64,
    /// Als Broadcast verteilte Frames.
    pub frames_broadcast: u64,
    /// Frames mit unbekanntem Ziel, die geflutet wurden.
    pub frames_unknown_unicast: u64,
    /// Gestellte ARP-Anfragen.
    pub arp_requests: u64,
    /// Beantwortete ARP-Anfragen.
    pub arp_replies: u64,
}

impl Bridge {
    /// Legt eine Bridge ohne Subnetz an.
    pub fn new(name: &str, _ns: NetNsId) -> Self {
        let mut iface = NetInterface::new_bridge(name);
        iface.up = true;
        Self {
            id: iface.id,
            name: name.to_string(),
            iface,
            ports: BTreeSet::new(),
            stp_enabled: false,
            stp_root: None,
            arp_table: BTreeMap::new(),
            mac_table: BTreeMap::new(),
            subnet: None,
            gateway_ip: None,
            dhcp_pool: None,
            created_at: 0,
            stats: BridgeStats::default(),
        }
    }
    /// Setzt Subnetz und Gateway der Bridge.
    pub fn set_subnet(&mut self, s: IpSubnet, gw: Ipv4Addr) {
        self.subnet = Some(s);
        self.gateway_ip = Some(gw);
        self.iface.ip = gw;
        self.iface.prefix_len = s.prefix_len;
    }
    /// Aktiviert den DHCP-Bereich zwischen zwei Adressen.
    pub fn enable_dhcp(&mut self, start: Ipv4Addr, end: Ipv4Addr, lease_h: u32) {
        self.dhcp_pool = Some(DhcpPool {
            start,
            end,
            lease_hours: lease_h,
            allocated: BTreeMap::new(),
            next_lease: start,
        });
    }
    /// Fuegt einen Port hinzu; false, wenn er schon eingetragen ist.
    pub fn add_port(&mut self, p: u64) -> bool {
        self.ports.insert(p)
    }
    /// Entfernt einen Port; false, wenn er nicht eingetragen war.
    pub fn remove_port(&mut self, p: u64) -> bool {
        self.ports.remove(&p)
    }
    /// Traegt eine MAC-Adresse auf einen Port ein.
    pub fn learn_mac(&mut self, mac: MacAddr, port: u64) {
        self.mac_table.insert(mac, port);
    }
    /// Port zu einer MAC-Adresse.
    pub fn lookup_port(&self, mac: MacAddr) -> Option<u64> {
        self.mac_table.get(&mac).copied()
    }
    /// MAC-Adresse zu einer bekannten IP.
    pub fn arp_resolve(&self, ip: Ipv4Addr) -> Option<MacAddr> {
        self.arp_table.get(&ip).copied()
    }
    /// Traegt eine Zuordnung von IP zu MAC ein.
    pub fn arp_learn(&mut self, ip: Ipv4Addr, mac: MacAddr) {
        self.arp_table.insert(ip, mac);
        self.stats.arp_replies += 1;
    }
    /// Leitet einen Frame weiter und liefert die Entscheidung.
    pub fn forward_frame(&mut self, dst: MacAddr, src: MacAddr, in_port: u64) -> ForwardDecision {
        self.learn_mac(src, in_port);
        if dst.is_broadcast() {
            self.stats.frames_broadcast += 1;
            return ForwardDecision::Flood;
        }
        match self.lookup_port(dst) {
            Some(op) if op != in_port => {
                self.stats.frames_forwarded += 1;
                ForwardDecision::Forward(op)
            }
            Some(op) if op == in_port => {
                self.stats.frames_dropped += 1;
                ForwardDecision::Drop
            }
            None => {
                self.stats.frames_unknown_unicast += 1;
                self.stats.frames_broadcast += 1;
                ForwardDecision::Flood
            }
            _ => {
                self.stats.frames_dropped += 1;
                ForwardDecision::Drop
            }
        }
    }
    /// Aktiviert Spanning Tree mit gewaehlter Wurzel.
    pub fn enable_stp(&mut self, root: u64) {
        self.stp_enabled = true;
        self.stp_root = Some(root);
    }
    /// Schaltet Spanning Tree ab.
    pub fn disable_stp(&mut self) {
        self.stp_enabled = false;
        self.stp_root = None;
    }
    /// Vergibt eine Adresse an einen Container, solange der Bereich reicht.
    pub fn allocate_ip(&mut self, cid: u32) -> Option<Ipv4Addr> {
        self.dhcp_pool.as_mut()?.allocate(cid)
    }
    /// Gibt eine Adresse frei; false, wenn sie nicht vergeben war.
    pub fn release_ip(&mut self, ip: Ipv4Addr) -> bool {
        self.dhcp_pool.as_mut().is_some_and(|p| p.release(ip))
    }
    /// Vergebene Adressen mit Container, Vergabe- und Ablaufzeit.
    pub fn leased_ips(&self) -> Vec<(Ipv4Addr, u32, u64)> {
        self.dhcp_pool.as_ref().map_or(vec![], |p| {
            p.allocated
                .iter()
                .map(|(&ip, l)| (ip, l.container_id, l.expires_at))
                .collect()
        })
    }
}

/// DHCP-Bereich einer Bridge.
#[derive(Clone, Debug)]
pub struct DhcpPool {
    /// Erste Adresse des Bereichs.
    pub start: Ipv4Addr,
    /// Letzte Adresse des Bereichs.
    pub end: Ipv4Addr,
    /// Leasedauer in Stunden.
    pub lease_hours: u32,
    /// Vergebene Adressen mit ihren Leases.
    pub allocated: BTreeMap<Ipv4Addr, DhcpLease>,
    /// Naechste Adresse fuer die Vergabe.
    pub next_lease: Ipv4Addr,
}

/// Ein vergebenes Lease.
#[derive(Clone, Debug)]
pub struct DhcpLease {
    /// Container, der die Adresse haelt.
    pub container_id: u32,
    /// Zeitpunkt der Vergabe.
    pub leased_at: u64,
    /// Zeitpunkt des Ablaufs.
    pub expires_at: u64,
}

impl DhcpPool {
    /// Anzahl der Adressen im Bereich.
    pub fn capacity(&self) -> u64 {
        let s = self.start.to_u32();
        let e = self.end.to_u32();
        if e >= s { (e - s + 1) as u64 } else { 0 }
    }
    /// Vergibt die naechste freie Adresse; abgelaufene Leases werden dabei uebersprungen.
    pub fn allocate(&mut self, cid: u32) -> Option<Ipv4Addr> {
        let s = self.start.to_u32();
        let e = self.end.to_u32();
        let mut c = self.next_lease.to_u32();
        loop {
            if c > e {
                c = s;
            }
            let ip = Ipv4Addr::from_u32(c);
            if !self.allocated.contains_key(&ip) {
                let now = 0u64;
                self.allocated.insert(
                    ip,
                    DhcpLease {
                        container_id: cid,
                        leased_at: now,
                        expires_at: now + (self.lease_hours as u64) * 3600,
                    },
                );
                self.next_lease = Ipv4Addr::from_u32(c + 1);
                return Some(ip);
            }
            c += 1;
            if c == self.next_lease.to_u32() {
                return None;
            }
        }
    }
    /// Gibt ein Lease frei; false, wenn die Adresse nicht vergeben war.
    pub fn release(&mut self, ip: Ipv4Addr) -> bool {
        self.allocated.remove(&ip).is_some()
    }
    /// True, wenn das Lease zum Zeitpunkt `now` abgelaufen ist.
    pub fn is_expired(&self, ip: Ipv4Addr, now: u64) -> bool {
        self.allocated.get(&ip).is_none_or(|l| now > l.expires_at)
    }
    /// Entfernt abgelaufene Leases und liefert deren Anzahl.
    pub fn cleanup_expired(&mut self, now: u64) -> u32 {
        let b = self.allocated.len();
        self.allocated.retain(|_, l| l.expires_at > now);
        (b - self.allocated.len()) as u32
    }
}

/// Entscheidung der Bridge fuer einen Frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForwardDecision {
    /// Frame an einen Port weiterleiten.
    Forward(u64),
    /// Frame an alle Ports ausser dem Eingang verteilen.
    Flood,
    /// Frame verwerfen.
    Drop,
}

// ══════════════════════════════════════════════════════════════════════════════
// FIREWALL (nftables-style)
// ══════════════════════════════════════════════════════════════════════════════

/// Kette einer Firewall.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum FirewallChain {
    /// Eingehende Pakete.
    Input,
    /// Ausgehende Pakete.
    Output,
    /// Weitergeleitete Pakete.
    Forward,
}
impl FirewallChain {
    /// Kurzname der Kette.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "INPUT",
            Self::Output => "OUTPUT",
            Self::Forward => "FORWARD",
        }
    }
}

/// Urteil einer Firewall-Regel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FirewallVerdict {
    /// Durchlassen.
    Accept,
    /// Verwerfen ohne Antwort.
    Drop,
    /// Verwerfen mit Ablehnung.
    Reject,
    /// Protokollieren und weiter pruefen.
    Log,
    /// Pruefung an den Aufrufer zurueckgeben.
    Return,
    /// In eine andere Kette springen.
    Jump(u64),
}

/// Transportprotokoll einer Regel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Protocol {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
    /// ICMP.
    Icmp,
    /// Beliebiges Protokoll.
    Any,
}
impl Protocol {
    /// Kurzname des Protokolls.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Icmp => "icmp",
            Self::Any => "any",
        }
    }
}

/// Eine Firewall-Regel mit Treffern und Zaehlern.
#[derive(Clone, Debug)]
pub struct FirewallRule {
    /// Kennung der Regel.
    pub id: u64,
    /// Kette der Regel.
    pub chain: FirewallChain,
    /// Namespace, in dem die Regel gilt.
    pub ns_id: NetNsId,
    /// Transportprotokoll.
    pub protocol: Protocol,
    /// Quelladresse, falls eingeschraenkt.
    pub src_ip: Option<Ipv4Addr>,
    /// Praefixlaenge der Quelladresse.
    pub src_mask: Option<u8>,
    /// Zieladresse, falls eingeschraenkt.
    pub dst_ip: Option<Ipv4Addr>,
    /// Praefixlaenge der Zieladresse.
    pub dst_mask: Option<u8>,
    /// Quellport, falls eingeschraenkt.
    pub src_port: Option<u16>,
    /// Zielport, falls eingeschraenkt.
    pub dst_port: Option<u16>,
    /// Bereich von Quellports.
    pub src_port_range: Option<(u16, u16)>,
    /// Bereich von Zielports.
    pub dst_port_range: Option<(u16, u16)>,
    /// Eingangsschnittstelle, falls eingeschraenkt.
    pub iface_in: Option<String>,
    /// Ausgangsschnittstelle, falls eingeschraenkt.
    pub iface_out: Option<String>,
    /// Urteil der Regel.
    pub verdict: FirewallVerdict,
    /// Prioritaet; hoehere Werte werden zuerst geprueft.
    pub priority: i32,
    /// Anzahl der Treffer.
    pub counter_hits: u64,
    /// Summe der durchgelassenen Bytes.
    pub counter_bytes: u64,
    /// Praefix fuer die Protokollzeile.
    pub log_prefix: Option<String>,
    /// Freier Kommentar.
    pub comment: Option<String>,
}

impl FirewallRule {
    /// Regel, die alles durchlaesst.
    pub fn accept(ns: NetNsId, c: FirewallChain) -> Self {
        Self::new(ns, c, FirewallVerdict::Accept)
    }
    /// Regel, die alles verwirft.
    pub fn drop(ns: NetNsId, c: FirewallChain) -> Self {
        Self::new(ns, c, FirewallVerdict::Drop)
    }
    /// Regel mit Urteil und ohne Einschraenkungen.
    pub fn new(ns: NetNsId, c: FirewallChain, v: FirewallVerdict) -> Self {
        Self {
            id: next_rule_id(),
            chain: c,
            ns_id: ns,
            protocol: Protocol::Any,
            src_ip: None,
            src_mask: None,
            dst_ip: None,
            dst_mask: None,
            src_port: None,
            dst_port: None,
            src_port_range: None,
            dst_port_range: None,
            iface_in: None,
            iface_out: None,
            verdict: v,
            priority: 100,
            counter_hits: 0,
            counter_bytes: 0,
            log_prefix: None,
            comment: None,
        }
    }
    /// Schraenkt auf ein Protokoll ein.
    pub fn with_protocol(mut self, p: Protocol) -> Self {
        self.protocol = p;
        self
    }
    /// Schraenkt auf ein Quellnetz ein.
    pub fn with_src(mut self, ip: Ipv4Addr, m: u8) -> Self {
        self.src_ip = Some(ip);
        self.src_mask = Some(m);
        self
    }
    /// Schraenkt auf ein Zielnetz ein.
    pub fn with_dst(mut self, ip: Ipv4Addr, m: u8) -> Self {
        self.dst_ip = Some(ip);
        self.dst_mask = Some(m);
        self
    }
    /// Schraenkt auf einen Zielport ein.
    pub fn with_dst_port(mut self, p: u16) -> Self {
        self.dst_port = Some(p);
        self
    }
    /// Schraenkt auf einen Zielportbereich ein.
    pub fn with_dst_port_range(mut self, a: u16, b: u16) -> Self {
        self.dst_port_range = Some((a, b));
        self
    }
    /// Schraenkt auf einen Quellport ein.
    pub fn with_src_port(mut self, p: u16) -> Self {
        self.src_port = Some(p);
        self
    }
    /// Schraenkt auf eine Eingangsschnittstelle ein.
    pub fn with_iface_in(mut self, n: &str) -> Self {
        self.iface_in = Some(n.to_string());
        self
    }
    /// Schraenkt auf eine Ausgangsschnittstelle ein.
    pub fn with_iface_out(mut self, n: &str) -> Self {
        self.iface_out = Some(n.to_string());
        self
    }
    /// Setzt die Prioritaet.
    pub fn with_priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }
    /// Setzt das Protokollpraefix.
    pub fn with_log(mut self, p: &str) -> Self {
        self.log_prefix = Some(p.to_string());
        self
    }
    /// Setzt den Kommentar.
    pub fn with_comment(mut self, c: &str) -> Self {
        self.comment = Some(c.to_string());
        self
    }

    // Bewusste Ausnahme: die Parameter spiegeln Paketfelder eins zu eins.
    // Sie in eine Struktur zu buendeln waere eine eigene API-Entscheidung und
    // gehoert nicht in die Portierung dieses Moduls.
    /// Prueft, ob ein Paket zu dieser Regel passt; zaehlt den Treffer dabei nicht.
    #[allow(clippy::too_many_arguments)]
    pub fn matches(
        &self,
        ns: NetNsId,
        chain: FirewallChain,
        proto: Protocol,
        src: Ipv4Addr,
        dst: Ipv4Addr,
        sp: Option<u16>,
        dp: Option<u16>,
        iin: Option<&str>,
        iout: Option<&str>,
    ) -> bool {
        if self.ns_id != ns || self.chain != chain {
            return false;
        }
        if self.protocol != Protocol::Any && self.protocol != proto {
            return false;
        }
        if let Some(r) = self.src_ip {
            let s = IpSubnet::new(r, self.src_mask.unwrap_or(32));
            if !s.contains(src) {
                return false;
            }
        }
        if let Some(r) = self.dst_ip {
            let s = IpSubnet::new(r, self.dst_mask.unwrap_or(32));
            if !s.contains(dst) {
                return false;
            }
        }
        if let Some(rp) = self.src_port {
            if sp != Some(rp) {
                return false;
            }
        }
        if let Some((lo, hi)) = self.src_port_range {
            match sp {
                Some(p) if p >= lo && p <= hi => {}
                _ => return false,
            }
        }
        if let Some(rp) = self.dst_port {
            if dp != Some(rp) {
                return false;
            }
        }
        if let Some((lo, hi)) = self.dst_port_range {
            match dp {
                Some(p) if p >= lo && p <= hi => {}
                _ => return false,
            }
        }
        if let Some(ref i) = self.iface_in {
            match iin {
                Some(n) if n == i => {}
                _ => return false,
            }
        }
        if let Some(ref i) = self.iface_out {
            match iout {
                Some(n) if n == i => {}
                _ => return false,
            }
        }
        true
    }
}

/// Eine Firewall-Kette mit Regeln und Zaehlern.
#[derive(Clone, Debug)]
pub struct FirewallChainData {
    /// Name der Kette.
    pub name: FirewallChain,
    /// Namespace der Kette.
    pub ns_id: NetNsId,
    /// Regeln der Kette.
    pub rules: Vec<FirewallRule>,
    /// Urteil, wenn keine Regel greift.
    pub default_policy: FirewallVerdict,
    /// Summe der Treffer.
    pub total_hits: u64,
    /// Verworfene Pakete.
    pub total_dropped: u64,
    /// Durchgelassene Pakete.
    pub total_accepted: u64,
}

impl FirewallChainData {
    /// Leere Kette mit Urteil `Drop` als Vorgabe.
    pub fn new(name: FirewallChain, ns: NetNsId) -> Self {
        Self {
            name,
            ns_id: ns,
            rules: vec![],
            default_policy: FirewallVerdict::Accept,
            total_hits: 0,
            total_dropped: 0,
            total_accepted: 0,
        }
    }
    /// Fuegt eine Regel hinzu; die Reihenfolge folgt der Prioritaet.
    pub fn add_rule(&mut self, r: FirewallRule) {
        self.rules.push(r);
        self.rules.sort_by_key(|r| r.priority);
    }
    /// Entfernt eine Regel; false, wenn sie nicht vorhanden war.
    pub fn remove_rule(&mut self, id: u64) -> bool {
        let b = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() < b
    }
    /// Setzt das Urteil fuer nicht passende Pakete.
    pub fn set_policy(&mut self, v: FirewallVerdict) {
        self.default_policy = v;
    }
    // Bewusste Ausnahme: die Parameter spiegeln Paketfelder eins zu eins.
    // Sie in eine Struktur zu buendeln waere eine eigene API-Entscheidung und
    // gehoert nicht in die Portierung dieses Moduls.
    /// Prueft ein Paket gegen die Kette und liefert das Urteil; Zaehler werden dabei fortgeschrieben.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        &mut self,
        proto: Protocol,
        src: Ipv4Addr,
        dst: Ipv4Addr,
        sp: Option<u16>,
        dp: Option<u16>,
        iin: Option<&str>,
        iout: Option<&str>,
    ) -> FirewallVerdict {
        self.total_hits += 1;
        for rule in self.rules.iter_mut() {
            if rule.matches(self.ns_id, self.name, proto, src, dst, sp, dp, iin, iout) {
                rule.counter_hits += 1;
                match rule.verdict {
                    FirewallVerdict::Accept => {
                        self.total_accepted += 1;
                        return FirewallVerdict::Accept;
                    }
                    FirewallVerdict::Drop => {
                        self.total_dropped += 1;
                        return FirewallVerdict::Drop;
                    }
                    FirewallVerdict::Reject => {
                        self.total_dropped += 1;
                        return FirewallVerdict::Reject;
                    }
                    FirewallVerdict::Log => continue,
                    FirewallVerdict::Return => return self.default_policy,
                    FirewallVerdict::Jump(_) => continue,
                }
            }
        }
        match self.default_policy {
            FirewallVerdict::Accept => self.total_accepted += 1,
            FirewallVerdict::Drop | FirewallVerdict::Reject => self.total_dropped += 1,
            _ => {}
        }
        self.default_policy
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// PORT FORWARDING (NAT)
// ══════════════════════════════════════════════════════════════════════════════

/// Art der Adressumsetzung.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum NatType {
    /// Zieladresse und Zielport umschreiben.
    Dnat,
    /// Quelladresse umschreiben.
    Snat,
    /// Quelladresse auf die externe Adresse umschreiben.
    Masquerade,
}
impl NatType {
    /// Kurzname der Art.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dnat => "DNAT",
            Self::Snat => "SNAT",
            Self::Masquerade => "MASQUERADE",
        }
    }
}

/// Eine Weiterleitung von einem Hostport auf einen Container.
#[derive(Clone, Debug)]
pub struct PortForward {
    /// Kennung der Weiterleitung.
    pub id: u64,
    /// Art der Umsetzung.
    pub nat_type: NatType,
    /// Namespace des Containers.
    pub ns_id: NetNsId,
    /// Adresse auf der Hostseite.
    pub host_ip: Ipv4Addr,
    /// Port auf der Hostseite.
    pub host_port: u16,
    /// Adresse des Containers.
    pub container_ip: Ipv4Addr,
    /// Port im Container.
    pub container_port: u16,
    /// Transportprotokoll.
    pub protocol: Protocol,
    /// True, solange die Weiterleitung aktiv ist.
    pub active: bool,
    /// Anzahl der Verbindungen.
    pub connections: u64,
    /// Weitergeleitete Bytes.
    pub bytes_forwarded: u64,
}

impl PortForward {
    /// Weiterleitung von einem Hostport auf einen Containerport.
    pub fn dnat(
        ns: NetNsId,
        hip: Ipv4Addr,
        hp: u16,
        cip: Ipv4Addr,
        cp: u16,
        proto: Protocol,
    ) -> Self {
        Self {
            id: next_portfwd_id(),
            nat_type: NatType::Dnat,
            ns_id: ns,
            host_ip: hip,
            host_port: hp,
            container_ip: cip,
            container_port: cp,
            protocol: proto,
            active: true,
            connections: 0,
            bytes_forwarded: 0,
        }
    }
    /// Umsetzung der Quelladresse auf die externe Adresse.
    pub fn masquerade(ns: NetNsId, cip: Ipv4Addr) -> Self {
        Self {
            id: next_portfwd_id(),
            nat_type: NatType::Masquerade,
            ns_id: ns,
            host_ip: Ipv4Addr::zero(),
            host_port: 0,
            container_ip: cip,
            container_port: 0,
            protocol: Protocol::Any,
            active: true,
            connections: 0,
            bytes_forwarded: 0,
        }
    }
    /// Zieladresse und Zielport nach der Umsetzung.
    pub fn translate_dst(&self) -> (Ipv4Addr, u16) {
        (self.container_ip, self.container_port)
    }
    /// Quelladresse und Quellport nach der Umsetzung.
    pub fn translate_src(&self, ext: Ipv4Addr) -> (Ipv4Addr, u16) {
        (ext, self.host_port)
    }
    /// Zaehlt eine Verbindung und ihre Bytes.
    pub fn record_connection(&mut self, bytes: u64) {
        self.connections += 1;
        self.bytes_forwarded += bytes;
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// NAMESPACE STATS
// ══════════════════════════════════════════════════════════════════════════════

/// Zaehler eines Namespace ueber alle Schnittstellen.
#[derive(Clone, Debug, Default)]
pub struct NetNsStats {
    /// Empfangene Bytes.
    pub total_rx_bytes: u64,
    /// Gesendete Bytes.
    pub total_tx_bytes: u64,
    /// Empfangene Pakete.
    pub total_rx_packets: u64,
    /// Gesendete Pakete.
    pub total_tx_packets: u64,
    /// Empfangsfehler.
    pub total_rx_errors: u64,
    /// Sendefehler.
    pub total_tx_errors: u64,
}
impl NetNsStats {
    /// Summe aus empfangenen und gesendeten Bytes.
    pub fn total_bytes(&self) -> u64 {
        self.total_rx_bytes + self.total_tx_bytes
    }
    /// Summe aus empfangenen und gesendeten Paketen.
    pub fn total_packets(&self) -> u64 {
        self.total_rx_packets + self.total_tx_packets
    }
    /// Anteil der Fehler an allen Paketen.
    pub fn error_rate(&self) -> f64 {
        let t = self.total_rx_packets + self.total_tx_packets;
        if t == 0 {
            0.0
        } else {
            (self.total_rx_errors + self.total_tx_errors) as f64 / t as f64
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// CONTAINER NETWORK MANAGER
// ══════════════════════════════════════════════════════════════════════════════

/// Verwaltung aller Namespaces, Bridges, Paare, Weiterleitungen, Regeln und Namen.
#[derive(Clone, Debug)]
pub struct ContainerNetManager {
    /// Namespaces nach Kennung.
    pub namespaces: BTreeMap<NetNsId, NetworkNamespace>,
    /// Bridges nach Kennung.
    pub bridges: BTreeMap<u64, Bridge>,
    /// veth-Paare nach Kennung.
    pub veth_pairs: BTreeMap<u64, VethPair>,
    /// Portweiterleitungen nach Kennung.
    pub port_forwards: BTreeMap<u64, PortForward>,
    /// Firewall-Ketten je Namespace und Kette.
    pub firewall_chains: BTreeMap<(NetNsId, FirewallChain), FirewallChainData>,
    /// Namenseintraege nach Kennung.
    pub dns_entries: BTreeMap<u64, DnsEntry>,
    /// Namenseintraege nach Hostname.
    pub dns_by_name: BTreeMap<String, DnsEntry>,
    /// Subnetz fuer neu angelegte Bridges.
    pub default_subnet: IpSubnet,
    /// Gateway fuer neu angelegte Bridges.
    pub default_gateway: Ipv4Addr,
    /// Externe Adresse fuer die Umsetzung nach aussen.
    pub external_ip: Ipv4Addr,
    /// Naechste freie Bridge-Kennung.
    pub bridge_counter: u64,
}

impl ContainerNetManager {
    /// Verwaltung mit Vorgaben aus dem Bereich 10.88.0.0/16.
    pub fn new() -> Self {
        Self {
            namespaces: BTreeMap::new(),
            bridges: BTreeMap::new(),
            veth_pairs: BTreeMap::new(),
            port_forwards: BTreeMap::new(),
            firewall_chains: BTreeMap::new(),
            dns_entries: BTreeMap::new(),
            dns_by_name: BTreeMap::new(),
            default_subnet: IpSubnet::new(Ipv4Addr::new(10, 42, 0, 0), 24),
            default_gateway: Ipv4Addr::new(10, 42, 0, 1),
            external_ip: Ipv4Addr::new(192, 168, 1, 100),
            bridge_counter: 0,
        }
    }

    /// Legt einen Namespace an und liefert seine Kennung.
    pub fn create_namespace(&mut self, name: &str, pid: u32) -> NetNsId {
        let ns = NetworkNamespace::new(name, pid);
        let id = ns.id;
        self.namespaces.insert(id, ns);
        for c in [
            FirewallChain::Input,
            FirewallChain::Output,
            FirewallChain::Forward,
        ] {
            self.firewall_chains
                .insert((id, c), FirewallChainData::new(c, id));
        }
        id
    }
    /// Loescht einen Namespace samt Schnittstellen und Ketten.
    pub fn destroy_namespace(&mut self, ns: NetNsId) -> bool {
        self.veth_pairs
            .retain(|_, vp| vp.host_ns != ns && vp.container_ns != ns);
        self.port_forwards.retain(|_, pf| pf.ns_id != ns);
        self.firewall_chains.retain(|(n, _), _| *n != ns);
        let ids: Vec<u64> = self
            .dns_entries
            .iter()
            .filter(|(_, e)| e.namespace == ns)
            .map(|(i, _)| *i)
            .collect();
        for id in ids {
            if let Some(e) = self.dns_entries.remove(&id) {
                self.dns_by_name.remove(&e.hostname);
            }
        }
        self.namespaces.remove(&ns).is_some()
    }
    /// Namespace nach Kennung.
    pub fn get_namespace(&self, ns: NetNsId) -> Option<&NetworkNamespace> {
        self.namespaces.get(&ns)
    }
    /// Namespace nach Kennung, veraenderbar.
    pub fn get_namespace_mut(&mut self, ns: NetNsId) -> Option<&mut NetworkNamespace> {
        self.namespaces.get_mut(&ns)
    }
    /// Alle Namespaces.
    pub fn list_namespaces(&self) -> Vec<&NetworkNamespace> {
        self.namespaces.values().collect()
    }

    /// Legt eine Bridge mit Subnetz, Gateway und DHCP an.
    pub fn create_bridge(&mut self, name: &str, host_ns: NetNsId) -> u64 {
        let mut b = Bridge::new(name, host_ns);
        b.set_subnet(self.default_subnet, self.default_gateway);
        b.enable_dhcp(
            Ipv4Addr::new(10, 42, 0, 10),
            Ipv4Addr::new(10, 42, 0, 254),
            24,
        );
        if let Some(ns) = self.namespaces.get_mut(&host_ns) {
            ns.add_interface(b.iface.clone());
        }
        let id = b.id;
        self.bridges.insert(id, b);
        self.bridge_counter += 1;
        id
    }
    /// Loescht eine Bridge.
    pub fn destroy_bridge(&mut self, bid: u64) -> bool {
        self.veth_pairs.retain(|_, vp| vp.bridge_iface != Some(bid));
        self.bridges.remove(&bid).is_some()
    }
    /// Bridge nach Kennung.
    pub fn get_bridge(&self, bid: u64) -> Option<&Bridge> {
        self.bridges.get(&bid)
    }
    /// Bridge nach Kennung, veraenderbar.
    pub fn get_bridge_mut(&mut self, bid: u64) -> Option<&mut Bridge> {
        self.bridges.get_mut(&bid)
    }

    /// Legt ein veth-Paar an, haengt das Hostende an die Bridge und vergibt die Containeradresse.
    pub fn create_veth_pair(
        &mut self,
        host_ns: NetNsId,
        c_ns: NetNsId,
        hn: &str,
        cn: &str,
        bridge: Option<u64>,
    ) -> Result<u64, String> {
        if !self.namespaces.contains_key(&host_ns) {
            return Err("host namespace not found".into());
        }
        if !self.namespaces.contains_key(&c_ns) {
            return Err("container namespace not found".into());
        }
        let mut veth = VethPair::new(hn, cn, host_ns, c_ns);
        if let Some(bid) = bridge {
            if let Some(b) = self.bridges.get_mut(&bid) {
                let cid = self.namespaces.get(&c_ns).unwrap().container_pid;
                if let Some(ip) = b.allocate_ip(cid) {
                    veth.set_container_ip(ip, b.subnet.unwrap_or(self.default_subnet).prefix_len);
                }
                veth.attach_to_bridge(bid);
                b.add_port(veth.container_iface.id);
            }
        }
        veth.bring_up();
        let vid = veth.id;
        if let Some(ns) = self.namespaces.get_mut(&c_ns) {
            ns.add_interface(veth.container_iface.clone());
            if let Some(bid) = bridge {
                if let Some(b) = self.bridges.get(&bid) {
                    if let Some(gw) = b.gateway_ip {
                        ns.add_route(Route::default(gw, veth.container_iface.id));
                    }
                }
            }
        }
        if let Some(ns) = self.namespaces.get_mut(&host_ns) {
            ns.add_interface(veth.host_iface.clone());
        }
        self.veth_pairs.insert(vid, veth);
        Ok(vid)
    }
    /// Loescht ein veth-Paar.
    pub fn destroy_veth_pair(&mut self, vid: u64) -> bool {
        if let Some(veth) = self.veth_pairs.remove(&vid) {
            if let Some(bid) = veth.bridge_iface {
                if let Some(b) = self.bridges.get_mut(&bid) {
                    b.remove_port(veth.container_iface.id);
                    b.release_ip(veth.container_iface.ip);
                }
            }
            if let Some(ns) = self.namespaces.get_mut(&veth.container_ns) {
                ns.remove_interface(veth.container_iface.id);
            }
            if let Some(ns) = self.namespaces.get_mut(&veth.host_ns) {
                ns.remove_interface(veth.host_iface.id);
            }
            true
        } else {
            false
        }
    }

    /// Legt eine Weiterleitung von Hostport auf Containerport an.
    pub fn add_port_forward(
        &mut self,
        ns: NetNsId,
        hp: u16,
        cip: Ipv4Addr,
        cp: u16,
        proto: Protocol,
    ) -> u64 {
        let pf = PortForward::dnat(ns, self.external_ip, hp, cip, cp, proto);
        let id = pf.id;
        self.port_forwards.insert(id, pf);
        id
    }
    /// Entfernt eine Weiterleitung.
    pub fn remove_port_forward(&mut self, id: u64) -> bool {
        self.port_forwards.remove(&id).is_some()
    }
    /// Legt eine Umsetzung der Quelladresse an.
    pub fn add_masquerade(&mut self, ns: NetNsId, cip: Ipv4Addr) -> u64 {
        let pf = PortForward::masquerade(ns, cip);
        let id = pf.id;
        self.port_forwards.insert(id, pf);
        id
    }
    /// Weiterleitung zu einem Hostport und Protokoll.
    pub fn lookup_port_forward(&self, hp: u16, proto: Protocol) -> Option<&PortForward> {
        self.port_forwards
            .values()
            .find(|pf| pf.host_port == hp && pf.protocol == proto && pf.active)
    }

    /// Traegt eine Firewall-Regel ein; false, wenn die Kette sie ablehnt.
    pub fn add_firewall_rule(&mut self, r: FirewallRule) -> bool {
        if let Some(c) = self.firewall_chains.get_mut(&(r.ns_id, r.chain)) {
            c.add_rule(r);
            true
        } else {
            false
        }
    }
    /// Entfernt eine Firewall-Regel.
    pub fn remove_firewall_rule(&mut self, id: u64) -> bool {
        for c in self.firewall_chains.values_mut() {
            if c.remove_rule(id) {
                return true;
            }
        }
        false
    }
    /// Setzt das Urteil einer Kette fuer nicht passende Pakete.
    pub fn set_firewall_policy(
        &mut self,
        ns: NetNsId,
        c: FirewallChain,
        v: FirewallVerdict,
    ) -> bool {
        if let Some(ch) = self.firewall_chains.get_mut(&(ns, c)) {
            ch.set_policy(v);
            true
        } else {
            false
        }
    }
    // Bewusste Ausnahme: die Parameter spiegeln Paketfelder eins zu eins.
    // Sie in eine Struktur zu buendeln waere eine eigene API-Entscheidung und
    // gehoert nicht in die Portierung dieses Moduls.
    /// Prueft ein Paket gegen die Kette eines Namespace.
    #[allow(clippy::too_many_arguments)]
    pub fn check_firewall(
        &mut self,
        ns: NetNsId,
        c: FirewallChain,
        proto: Protocol,
        src: Ipv4Addr,
        dst: Ipv4Addr,
        sp: Option<u16>,
        dp: Option<u16>,
        iin: Option<&str>,
        iout: Option<&str>,
    ) -> FirewallVerdict {
        match self.firewall_chains.get_mut(&(ns, c)) {
            Some(ch) => ch.evaluate(proto, src, dst, sp, dp, iin, iout),
            None => FirewallVerdict::Accept,
        }
    }

    /// Traegt einen Namen fuer einen Namespace ein.
    pub fn register_dns(&mut self, host: &str, ip: Ipv4Addr, ns: NetNsId, ttl: u32) -> u64 {
        let e = DnsEntry {
            id: next_dns_id(),
            hostname: host.to_string(),
            ip,
            namespace: ns,
            ttl,
            created_at: 0,
        };
        let id = e.id;
        self.dns_by_name.insert(host.to_string(), e.clone());
        self.dns_entries.insert(id, e);
        id
    }
    /// Loest einen Namen in einem Namespace auf.
    pub fn resolve_dns(&self, host: &str, ns: NetNsId) -> Option<Ipv4Addr> {
        for e in self.dns_entries.values() {
            if e.hostname == host && e.namespace == ns {
                return Some(e.ip);
            }
        }
        self.dns_by_name.get(host).map(|e| e.ip)
    }
    /// Entfernt einen Namenseintrag.
    pub fn unregister_dns(&mut self, id: u64) -> bool {
        if let Some(e) = self.dns_entries.remove(&id) {
            self.dns_by_name.remove(&e.hostname);
            true
        } else {
            false
        }
    }

    // Bewusste Ausnahme: die Parameter spiegeln Paketfelder eins zu eins.
    // Sie in eine Struktur zu buendeln waere eine eigene API-Entscheidung und
    // gehoert nicht in die Portierung dieses Moduls.
    /// Simuliert ein Paket durch die Netzkette und liefert das Ergebnis.
    #[allow(clippy::too_many_arguments)]
    pub fn simulate_packet(
        &mut self,
        src_ns: NetNsId,
        dst_ns: NetNsId,
        src: Ipv4Addr,
        dst: Ipv4Addr,
        proto: Protocol,
        sp: Option<u16>,
        dp: Option<u16>,
        bytes: u64,
    ) -> PacketResult {
        let out_v = self.check_firewall(
            src_ns,
            FirewallChain::Output,
            proto,
            src,
            dst,
            sp,
            dp,
            None,
            None,
        );
        if out_v == FirewallVerdict::Drop || out_v == FirewallVerdict::Reject {
            return PacketResult::Dropped("OUTPUT chain dropped".into());
        }
        let in_v = self.check_firewall(
            dst_ns,
            FirewallChain::Input,
            proto,
            src,
            dst,
            sp,
            dp,
            None,
            None,
        );
        if in_v == FirewallVerdict::Drop || in_v == FirewallVerdict::Reject {
            return PacketResult::Dropped("INPUT chain dropped".into());
        }
        if let Some(pf) = self.port_forwards.values_mut().find(|pf| {
            pf.nat_type == NatType::Dnat
                && pf.host_port == dp.unwrap_or(0)
                && pf.protocol == proto
                && pf.active
        }) {
            pf.record_connection(bytes);
            return PacketResult::Forwarded {
                new_dst: (pf.container_ip, pf.container_port),
                bytes,
            };
        }
        if let Some(ns) = self.namespaces.get_mut(&src_ns) {
            if let Some(r) = ns.lookup_route(dst).cloned() {
                ns.tx(r.iface_id, bytes, 1);
            }
        }
        if let Some(ns) = self.namespaces.get_mut(&dst_ns) {
            ns.rx(0, bytes, 1);
        }
        PacketResult::Delivered { bytes }
    }

    /// Zaehler eines Namespace.
    pub fn namespace_stats(&self, ns: NetNsId) -> Option<&NetNsStats> {
        self.namespaces.get(&ns).map(|ns| &ns.stats)
    }
    /// Zaehler einer Bridge.
    pub fn bridge_stats(&self, bid: u64) -> Option<&BridgeStats> {
        self.bridges.get(&bid).map(|b| &b.stats)
    }
    /// Zaehler einer Firewall-Kette.
    pub fn firewall_stats(&self, ns: NetNsId, c: FirewallChain) -> Option<&FirewallChainData> {
        self.firewall_chains.get(&(ns, c))
    }
    /// Alle Weiterleitungen mit ihren Zaehlern.
    pub fn port_forward_stats(&self) -> Vec<&PortForward> {
        self.port_forwards.values().collect()
    }
    /// Anzahl der Namespaces.
    pub fn total_namespaces(&self) -> usize {
        self.namespaces.len()
    }
    /// Anzahl der Bridges.
    pub fn total_bridges(&self) -> usize {
        self.bridges.len()
    }
    /// Anzahl der veth-Paare.
    pub fn total_veth_pairs(&self) -> usize {
        self.veth_pairs.len()
    }
    /// Anzahl der Portweiterleitungen.
    pub fn total_port_forwards(&self) -> usize {
        self.port_forwards.len()
    }
    /// Anzahl der Namenseintraege.
    pub fn total_dns_entries(&self) -> usize {
        self.dns_entries.len()
    }
    /// Anzahl der Firewall-Regeln ueber alle Ketten.
    pub fn total_firewall_rules(&self) -> usize {
        self.firewall_chains.values().map(|c| c.rules.len()).sum()
    }

    /// Bericht ueber Namespaces, Bridges, Paare, Weiterleitungen und Regeln.
    pub fn report(&self) -> String {
        let mut r = String::new();
        r.push_str("═══ Container Network Report ═══\n");
        r.push_str(&format!("Namespaces:      {}\n", self.total_namespaces()));
        r.push_str(&format!("Bridges:         {}\n", self.total_bridges()));
        r.push_str(&format!("Veth Pairs:      {}\n", self.total_veth_pairs()));
        r.push_str(&format!(
            "Port Forwards:   {}\n",
            self.total_port_forwards()
        ));
        r.push_str(&format!("DNS Entries:     {}\n", self.total_dns_entries()));
        r.push_str(&format!(
            "Firewall Rules:  {}\n",
            self.total_firewall_rules()
        ));
        r.push_str(&format!("Default Subnet:  {}\n", self.default_subnet));
        r.push_str(&format!("Gateway:         {}\n", self.default_gateway));
        r.push_str(&format!("External IP:     {}\n", self.external_ip));
        r.push_str("\n─── Namespaces ──\n");
        for ns in self.namespaces.values() {
            r.push_str(&format!(
                "  {} (pid {}): {} ifaces, rx={}B tx={}B\n",
                ns.name,
                ns.container_pid,
                ns.interfaces.len(),
                ns.stats.total_rx_bytes,
                ns.stats.total_tx_bytes
            ));
        }
        r.push_str("\n─── Bridges ──\n");
        for b in self.bridges.values() {
            r.push_str(&format!(
                "  {}: {} ports, fwd={} drop={} bcast={}\n",
                b.name,
                b.ports.len(),
                b.stats.frames_forwarded,
                b.stats.frames_dropped,
                b.stats.frames_broadcast
            ));
        }
        r.push_str("\n─── Port Forwards ──\n");
        for pf in self.port_forwards.values() {
            r.push_str(&format!(
                "  {} {}:{} → {}:{} ({} conns, {}B)\n",
                pf.nat_type.as_str(),
                pf.host_ip,
                pf.host_port,
                pf.container_ip,
                pf.container_port,
                pf.connections,
                pf.bytes_forwarded
            ));
        }
        r
    }
}

impl Default for ContainerNetManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Ergebnis einer Paketsimulation.
#[derive(Clone, Debug)]
pub enum PacketResult {
    /// Paket wurde im Ziel-Namespace abgeliefert.
    Delivered {
        /// Anzahl der abgelieferten Bytes.
        bytes: u64,
    },
    /// Paket wurde weitergeleitet.
    Forwarded {
        /// Ziel, an das das Paket weitergeleitet wurde.
        new_dst: (Ipv4Addr, u16),
        /// Anzahl der weitergeleiteten Bytes.
        bytes: u64,
    },
    /// Paket wurde verworfen; der Grund steht im Text.
    Dropped(String),
    /// Paket wurde abgelehnt; der Grund steht im Text.
    Rejected(String),
}

// ══════════════════════════════════════════════════════════════════════════════
// TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_ipv4_basic() {
        let ip = Ipv4Addr::new(192, 168, 1, 1);
        assert_eq!(ip.to_string(), "192.168.1.1");
        assert_eq!(ip.to_u32(), 0xC0A80101);
        assert!(!ip.is_zero());
        assert!(Ipv4Addr::zero().is_zero());
        assert_eq!(Ipv4Addr::from_u32(0xC0A80101), ip);
    }
    #[test]
    fn t_ipv4_ordering() {
        assert!(Ipv4Addr::new(10, 0, 0, 1) < Ipv4Addr::new(10, 0, 0, 2));
    }
    #[test]
    fn t_subnet_basic() {
        let n = IpSubnet::new(Ipv4Addr::new(10, 42, 0, 0), 24);
        assert!(n.contains(Ipv4Addr::new(10, 42, 0, 1)));
        assert!(n.contains(Ipv4Addr::new(10, 42, 0, 254)));
        assert!(!n.contains(Ipv4Addr::new(10, 43, 0, 1)));
        assert_eq!(n.broadcast_addr(), Ipv4Addr::new(10, 42, 0, 255));
    }
    #[test]
    fn t_subnet_capacity() {
        assert_eq!(
            IpSubnet::new(Ipv4Addr::new(10, 0, 0, 0), 24).host_capacity(),
            254
        );
        assert_eq!(
            IpSubnet::new(Ipv4Addr::new(10, 0, 0, 0), 30).host_capacity(),
            2
        );
        assert_eq!(
            IpSubnet::new(Ipv4Addr::new(10, 0, 0, 0), 32).host_capacity(),
            1
        );
        assert_eq!(
            IpSubnet::new(Ipv4Addr::new(10, 0, 0, 0), 16).host_capacity(),
            65534
        );
    }
    #[test]
    fn t_subnet_to_string() {
        assert_eq!(
            IpSubnet::new(Ipv4Addr::new(172, 16, 0, 0), 12).to_string(),
            "172.16.0.0/12"
        );
    }
    #[test]
    fn t_subnet_edge() {
        let n0 = IpSubnet::new(Ipv4Addr::new(0, 0, 0, 0), 0);
        assert!(n0.contains(Ipv4Addr::new(1, 2, 3, 4)));
        let n32 = IpSubnet::new(Ipv4Addr::new(10, 0, 0, 1), 32);
        assert!(n32.contains(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(!n32.contains(Ipv4Addr::new(10, 0, 0, 2)));
    }
    #[test]
    fn t_mac_basic() {
        let m = MacAddr::new(0x02, 0x42, 0xAC, 0x10, 0x00, 0x01);
        assert_eq!(m.to_string(), "02:42:ac:10:00:01");
        assert!(!m.is_broadcast());
        assert!(!m.is_zero());
    }
    #[test]
    fn t_mac_bcast_zero() {
        assert!(MacAddr::broadcast().is_broadcast());
        assert!(MacAddr::zero().is_zero());
    }
    #[test]
    fn t_mac_random() {
        let a = MacAddr::random();
        let b = MacAddr::random();
        assert_ne!(a, b);
        assert_eq!(a.0[0], 0x02);
        assert_eq!(a.0[1], 0x42);
    }
    #[test]
    fn t_ns_creation() {
        let ns = NetworkNamespace::new("test", 1000);
        assert_eq!(ns.name, "test");
        assert_eq!(ns.container_pid, 1000);
        assert_eq!(ns.interfaces.len(), 1);
        assert!(ns.loopback_up);
        assert_eq!(ns.routing_table.len(), 1);
    }
    #[test]
    fn t_ns_add_iface() {
        let mut ns = NetworkNamespace::new("t", 1);
        let v = NetInterface::new_veth("eth0");
        let id = v.id;
        ns.add_interface(v);
        assert_eq!(ns.interfaces.len(), 2);
        assert!(ns.get_interface(id).is_some());
    }
    #[test]
    fn t_ns_remove_iface() {
        let mut ns = NetworkNamespace::new("t", 1);
        let v = NetInterface::new_veth("eth0");
        let id = v.id;
        ns.add_interface(v);
        assert!(ns.remove_interface(id).is_some());
        assert_eq!(ns.interfaces.len(), 1);
    }
    #[test]
    fn t_ns_iface_up_down() {
        let mut ns = NetworkNamespace::new("t", 1);
        let v = NetInterface::new_veth("eth0");
        let id = v.id;
        ns.add_interface(v);
        assert!(ns.set_interface_up(id, true));
        assert!(ns.get_interface(id).unwrap().up);
        assert!(ns.set_interface_up(id, false));
        assert!(!ns.get_interface(id).unwrap().up);
    }
    #[test]
    fn t_ns_iface_ip() {
        let mut ns = NetworkNamespace::new("t", 1);
        let v = NetInterface::new_veth("eth0");
        let id = v.id;
        ns.add_interface(v);
        assert!(ns.set_interface_ip(id, Ipv4Addr::new(10, 42, 0, 5), 24));
        assert_eq!(
            ns.get_interface(id).unwrap().ip,
            Ipv4Addr::new(10, 42, 0, 5)
        );
    }
    #[test]
    fn t_ns_mtu() {
        let mut ns = NetworkNamespace::new("t", 1);
        let v = NetInterface::new_veth("eth0");
        let id = v.id;
        ns.add_interface(v);
        assert!(ns.set_mtu(id, 9000));
        assert!(!ns.set_mtu(id, 10));
        assert!(!ns.set_mtu(id, 70000));
    }
    #[test]
    fn t_ns_route() {
        let mut ns = NetworkNamespace::new("t", 1);
        ns.add_route(Route::default(Ipv4Addr::new(10, 42, 0, 1), 1));
        assert_eq!(ns.routing_table.len(), 2);
        assert!(ns.routing_table[1].is_default());
    }
    #[test]
    fn t_ns_lookup_route() {
        let mut ns = NetworkNamespace::new("t", 1);
        ns.add_route(Route {
            dest: IpSubnet::new(Ipv4Addr::new(10, 42, 0, 0), 24),
            gateway: Ipv4Addr::zero(),
            iface_id: 1,
            metric: 10,
        });
        ns.add_route(Route::default(Ipv4Addr::new(10, 42, 0, 1), 1));
        assert_eq!(
            ns.lookup_route(Ipv4Addr::new(10, 42, 0, 5))
                .unwrap()
                .dest
                .prefix_len,
            24
        );
        assert!(
            ns.lookup_route(Ipv4Addr::new(8, 8, 8, 8))
                .unwrap()
                .is_default()
        );
    }
    #[test]
    fn t_ns_remove_route() {
        let mut ns = NetworkNamespace::new("t", 1);
        let d = IpSubnet::new(Ipv4Addr::new(10, 42, 0, 0), 24);
        ns.add_route(Route {
            dest: d,
            gateway: Ipv4Addr::zero(),
            iface_id: 1,
            metric: 10,
        });
        assert!(ns.remove_route(d));
        assert_eq!(ns.routing_table.len(), 1);
    }
    #[test]
    fn t_ns_dns() {
        let mut ns = NetworkNamespace::new("t", 1);
        ns.set_dns(vec![Ipv4Addr::new(8, 8, 8, 8)], vec!["x.com".into()]);
        assert_eq!(ns.dns_config.servers.len(), 1);
    }
    #[test]
    fn t_ns_traffic() {
        let mut ns = NetworkNamespace::new("t", 1);
        ns.rx(0, 1000, 10);
        ns.tx(0, 500, 5);
        assert_eq!(ns.stats.total_rx_bytes, 1000);
        assert_eq!(ns.stats.total_tx_bytes, 500);
    }
    #[test]
    fn t_ns_errors() {
        let mut ns = NetworkNamespace::new("t", 1);
        ns.record_error(0, true);
        ns.record_error(0, false);
        ns.record_error(0, true);
        assert_eq!(ns.stats.total_rx_errors, 2);
        assert_eq!(ns.stats.total_tx_errors, 1);
    }
    #[test]
    fn t_veth_creation() {
        let v = VethPair::new("v0", "e0", NetNsId(1), NetNsId(2));
        assert_eq!(v.host_iface.name, "v0");
        assert_eq!(v.container_iface.name, "e0");
        assert_eq!(v.host_iface.peer_ns, Some(NetNsId(2)));
        assert_eq!(v.container_iface.peer_ns, Some(NetNsId(1)));
    }
    #[test]
    fn t_veth_ips() {
        let mut v = VethPair::new("v0", "e0", NetNsId(1), NetNsId(2));
        v.set_container_ip(Ipv4Addr::new(10, 42, 0, 5), 24);
        v.set_host_ip(Ipv4Addr::new(10, 42, 0, 1), 24);
        assert_eq!(v.container_iface.ip, Ipv4Addr::new(10, 42, 0, 5));
        assert_eq!(v.host_iface.ip, Ipv4Addr::new(10, 42, 0, 1));
    }
    #[test]
    fn t_veth_up_down() {
        let mut v = VethPair::new("v0", "e0", NetNsId(1), NetNsId(2));
        assert!(!v.host_iface.up);
        v.bring_up();
        assert!(v.host_iface.up);
        v.tear_down();
        assert!(!v.host_iface.up);
    }
    #[test]
    fn t_veth_bridge() {
        let mut v = VethPair::new("v0", "e0", NetNsId(1), NetNsId(2));
        v.attach_to_bridge(42);
        assert_eq!(v.bridge_iface, Some(42));
        v.detach_from_bridge();
        assert_eq!(v.bridge_iface, None);
    }
    #[test]
    fn t_bridge_creation() {
        let b = Bridge::new("br0", NetNsId(0));
        assert_eq!(b.name, "br0");
        assert!(b.iface.up);
        assert!(b.ports.is_empty());
    }
    #[test]
    fn t_bridge_subnet() {
        let mut b = Bridge::new("br0", NetNsId(0));
        b.set_subnet(
            IpSubnet::new(Ipv4Addr::new(10, 42, 0, 0), 24),
            Ipv4Addr::new(10, 42, 0, 1),
        );
        assert_eq!(b.iface.ip, Ipv4Addr::new(10, 42, 0, 1));
        assert_eq!(b.gateway_ip, Some(Ipv4Addr::new(10, 42, 0, 1)));
    }
    #[test]
    fn t_bridge_port() {
        let mut b = Bridge::new("br0", NetNsId(0));
        let v = NetInterface::new_veth("v0");
        assert!(b.add_port(v.id));
        assert!(b.ports.contains(&v.id));
        assert!(b.remove_port(v.id));
    }
    #[test]
    fn t_bridge_mac_learn() {
        let mut b = Bridge::new("br0", NetNsId(0));
        let m = MacAddr::new(2, 66, 172, 16, 0, 1);
        b.learn_mac(m, 42);
        assert_eq!(b.lookup_port(m), Some(42));
    }
    #[test]
    fn t_bridge_arp() {
        let mut b = Bridge::new("br0", NetNsId(0));
        let ip = Ipv4Addr::new(10, 42, 0, 5);
        let mac = MacAddr::new(2, 66, 172, 16, 0, 5);
        b.arp_learn(ip, mac);
        assert_eq!(b.arp_resolve(ip), Some(mac));
    }
    #[test]
    fn t_bridge_fwd_known() {
        let mut b = Bridge::new("br0", NetNsId(0));
        let s = MacAddr::new(2, 66, 172, 16, 0, 1);
        let d = MacAddr::new(2, 66, 172, 16, 0, 2);
        b.learn_mac(d, 2);
        assert_eq!(b.forward_frame(d, s, 1), ForwardDecision::Forward(2));
        assert_eq!(b.stats.frames_forwarded, 1);
    }
    #[test]
    fn t_bridge_fwd_bcast() {
        let mut b = Bridge::new("br0", NetNsId(0));
        let s = MacAddr::new(2, 66, 172, 16, 0, 1);
        assert_eq!(
            b.forward_frame(MacAddr::broadcast(), s, 1),
            ForwardDecision::Flood
        );
    }
    #[test]
    fn t_bridge_fwd_unknown() {
        let mut b = Bridge::new("br0", NetNsId(0));
        let s = MacAddr::new(2, 66, 172, 16, 0, 1);
        let u = MacAddr::new(2, 66, 172, 16, 0, 99);
        assert_eq!(b.forward_frame(u, s, 1), ForwardDecision::Flood);
        assert_eq!(b.stats.frames_unknown_unicast, 1);
    }
    #[test]
    fn t_bridge_fwd_same_port() {
        let mut b = Bridge::new("br0", NetNsId(0));
        let m = MacAddr::new(2, 66, 172, 16, 0, 1);
        b.learn_mac(m, 1);
        assert_eq!(b.forward_frame(m, m, 1), ForwardDecision::Drop);
    }
    #[test]
    fn t_bridge_stp() {
        let mut b = Bridge::new("br0", NetNsId(0));
        b.enable_stp(1);
        assert!(b.stp_enabled);
        assert_eq!(b.stp_root, Some(1));
        b.disable_stp();
        assert!(!b.stp_enabled);
    }
    #[test]
    fn t_dhcp_cap() {
        let p = DhcpPool {
            start: Ipv4Addr::new(10, 42, 0, 10),
            end: Ipv4Addr::new(10, 42, 0, 20),
            lease_hours: 24,
            allocated: BTreeMap::new(),
            next_lease: Ipv4Addr::new(10, 42, 0, 10),
        };
        assert_eq!(p.capacity(), 11);
    }
    #[test]
    fn t_dhcp_alloc_release() {
        let mut p = DhcpPool {
            start: Ipv4Addr::new(10, 42, 0, 10),
            end: Ipv4Addr::new(10, 42, 0, 15),
            lease_hours: 24,
            allocated: BTreeMap::new(),
            next_lease: Ipv4Addr::new(10, 42, 0, 10),
        };
        let a = p.allocate(100).unwrap();
        let b = p.allocate(101).unwrap();
        assert_ne!(a, b);
        assert!(p.release(a));
        assert!(!p.release(a));
    }
    #[test]
    fn t_dhcp_full() {
        let mut p = DhcpPool {
            start: Ipv4Addr::new(10, 42, 0, 10),
            end: Ipv4Addr::new(10, 42, 0, 12),
            lease_hours: 1,
            allocated: BTreeMap::new(),
            next_lease: Ipv4Addr::new(10, 42, 0, 10),
        };
        assert!(p.allocate(1).is_some());
        assert!(p.allocate(2).is_some());
        assert!(p.allocate(3).is_some());
        assert!(p.allocate(4).is_none());
    }
    #[test]
    fn t_dhcp_cleanup() {
        let mut p = DhcpPool {
            start: Ipv4Addr::new(10, 42, 0, 10),
            end: Ipv4Addr::new(10, 42, 0, 20),
            lease_hours: 1,
            allocated: BTreeMap::new(),
            next_lease: Ipv4Addr::new(10, 42, 0, 10),
        };
        p.allocate(1);
        p.allocate(2);
        if let Some(l) = p.allocated.values_mut().next() {
            l.expires_at = 100;
        }
        assert_eq!(p.cleanup_expired(200), 1);
        assert_eq!(p.allocated.len(), 1);
    }
    #[test]
    fn t_fw_rule_builder() {
        let r = FirewallRule::accept(NetNsId(1), FirewallChain::Input)
            .with_protocol(Protocol::Tcp)
            .with_dst(Ipv4Addr::new(10, 42, 0, 5), 32)
            .with_dst_port(80)
            .with_priority(10)
            .with_comment("HTTP");
        assert_eq!(r.chain, FirewallChain::Input);
        assert_eq!(r.protocol, Protocol::Tcp);
        assert_eq!(r.dst_port, Some(80));
        assert_eq!(r.priority, 10);
    }
    #[test]
    fn t_fw_match_exact() {
        let r = FirewallRule::accept(NetNsId(1), FirewallChain::Input)
            .with_protocol(Protocol::Tcp)
            .with_dst_port(80);
        assert!(r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Tcp,
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(10, 0, 0, 2),
            None,
            Some(80),
            None,
            None
        ));
    }
    #[test]
    fn t_fw_no_match_port() {
        let r = FirewallRule::drop(NetNsId(1), FirewallChain::Input).with_dst_port(22);
        assert!(!r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Tcp,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            Some(80),
            None,
            None
        ));
    }
    #[test]
    fn t_fw_no_match_ns() {
        let r = FirewallRule::accept(NetNsId(1), FirewallChain::Input).with_dst_port(80);
        assert!(!r.matches(
            NetNsId(2),
            FirewallChain::Input,
            Protocol::Tcp,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            Some(80),
            None,
            None
        ));
    }
    #[test]
    fn t_fw_subnet_match() {
        let r = FirewallRule::drop(NetNsId(1), FirewallChain::Input)
            .with_src(Ipv4Addr::new(10, 42, 0, 0), 24);
        assert!(r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Any,
            Ipv4Addr::new(10, 42, 0, 50),
            Ipv4Addr::new(192, 168, 1, 1),
            None,
            None,
            None,
            None
        ));
        assert!(!r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Any,
            Ipv4Addr::new(10, 43, 0, 50),
            Ipv4Addr::new(192, 168, 1, 1),
            None,
            None,
            None,
            None
        ));
    }
    #[test]
    fn t_fw_port_range() {
        let r =
            FirewallRule::accept(NetNsId(1), FirewallChain::Input).with_dst_port_range(8000, 9000);
        assert!(r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Tcp,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            Some(8080),
            None,
            None
        ));
        assert!(!r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Tcp,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            Some(7000),
            None,
            None
        ));
    }
    #[test]
    fn t_fw_iface() {
        let r = FirewallRule::accept(NetNsId(1), FirewallChain::Input).with_iface_in("eth0");
        assert!(r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Any,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            None,
            Some("eth0"),
            None
        ));
        assert!(!r.matches(
            NetNsId(1),
            FirewallChain::Input,
            Protocol::Any,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            None,
            Some("eth1"),
            None
        ));
    }
    #[test]
    fn t_fw_chain_add_remove() {
        let mut c = FirewallChainData::new(FirewallChain::Input, NetNsId(1));
        let r = FirewallRule::accept(NetNsId(1), FirewallChain::Input);
        let id = r.id;
        c.add_rule(r);
        assert_eq!(c.rules.len(), 1);
        assert!(c.remove_rule(id));
        assert_eq!(c.rules.len(), 0);
    }
    #[test]
    fn t_fw_chain_priority() {
        let mut c = FirewallChainData::new(FirewallChain::Input, NetNsId(1));
        c.add_rule(
            FirewallRule::new(NetNsId(1), FirewallChain::Input, FirewallVerdict::Accept)
                .with_priority(100),
        );
        c.add_rule(
            FirewallRule::new(NetNsId(1), FirewallChain::Input, FirewallVerdict::Drop)
                .with_priority(10),
        );
        assert_eq!(c.rules[0].priority, 10);
        assert_eq!(c.rules[1].priority, 100);
    }
    #[test]
    fn t_fw_eval_accept() {
        let mut c = FirewallChainData::new(FirewallChain::Input, NetNsId(1));
        c.add_rule(
            FirewallRule::accept(NetNsId(1), FirewallChain::Input)
                .with_protocol(Protocol::Tcp)
                .with_dst_port(80)
                .with_priority(10),
        );
        assert_eq!(
            c.evaluate(
                Protocol::Tcp,
                Ipv4Addr::new(10, 0, 0, 1),
                Ipv4Addr::new(10, 0, 0, 2),
                None,
                Some(80),
                None,
                None
            ),
            FirewallVerdict::Accept
        );
        assert_eq!(c.total_accepted, 1);
    }
    #[test]
    fn t_fw_eval_drop() {
        let mut c = FirewallChainData::new(FirewallChain::Input, NetNsId(1));
        c.add_rule(
            FirewallRule::drop(NetNsId(1), FirewallChain::Input)
                .with_protocol(Protocol::Tcp)
                .with_dst_port(22)
                .with_priority(5),
        );
        c.add_rule(FirewallRule::accept(NetNsId(1), FirewallChain::Input).with_priority(100));
        assert_eq!(
            c.evaluate(
                Protocol::Tcp,
                Ipv4Addr::zero(),
                Ipv4Addr::zero(),
                None,
                Some(22),
                None,
                None
            ),
            FirewallVerdict::Drop
        );
        assert_eq!(
            c.evaluate(
                Protocol::Tcp,
                Ipv4Addr::zero(),
                Ipv4Addr::zero(),
                None,
                Some(80),
                None,
                None
            ),
            FirewallVerdict::Accept
        );
    }
    #[test]
    fn t_fw_default_policy() {
        let mut c = FirewallChainData::new(FirewallChain::Input, NetNsId(1));
        c.set_policy(FirewallVerdict::Drop);
        assert_eq!(
            c.evaluate(
                Protocol::Any,
                Ipv4Addr::zero(),
                Ipv4Addr::zero(),
                None,
                None,
                None,
                None
            ),
            FirewallVerdict::Drop
        );
        assert_eq!(c.total_dropped, 1);
    }
    #[test]
    fn t_fw_counter() {
        let mut c = FirewallChainData::new(FirewallChain::Input, NetNsId(1));
        let r = FirewallRule::accept(NetNsId(1), FirewallChain::Input).with_protocol(Protocol::Tcp);
        let id = r.id;
        c.add_rule(r);
        c.evaluate(
            Protocol::Tcp,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            None,
            None,
            None,
        );
        c.evaluate(
            Protocol::Tcp,
            Ipv4Addr::zero(),
            Ipv4Addr::zero(),
            None,
            None,
            None,
            None,
        );
        assert_eq!(c.rules.iter().find(|r| r.id == id).unwrap().counter_hits, 2);
    }
    #[test]
    fn t_fw_log_verdict() {
        let mut c = FirewallChainData::new(FirewallChain::Input, NetNsId(1));
        c.add_rule(
            FirewallRule::new(NetNsId(1), FirewallChain::Input, FirewallVerdict::Log)
                .with_protocol(Protocol::Tcp)
                .with_priority(5)
                .with_log("LOG"),
        );
        c.add_rule(
            FirewallRule::accept(NetNsId(1), FirewallChain::Input)
                .with_protocol(Protocol::Tcp)
                .with_priority(10),
        );
        assert_eq!(
            c.evaluate(
                Protocol::Tcp,
                Ipv4Addr::zero(),
                Ipv4Addr::zero(),
                None,
                None,
                None,
                None
            ),
            FirewallVerdict::Accept
        );
    }
    #[test]
    fn t_pf_dnat() {
        let pf = PortForward::dnat(
            NetNsId(1),
            Ipv4Addr::new(192, 168, 1, 100),
            8080,
            Ipv4Addr::new(10, 42, 0, 5),
            80,
            Protocol::Tcp,
        );
        assert_eq!(pf.nat_type, NatType::Dnat);
        assert_eq!(pf.host_port, 8080);
        assert_eq!(pf.container_port, 80);
        assert!(pf.active);
    }
    #[test]
    fn t_pf_translate() {
        let pf = PortForward::dnat(
            NetNsId(1),
            Ipv4Addr::new(192, 168, 1, 100),
            8080,
            Ipv4Addr::new(10, 42, 0, 5),
            80,
            Protocol::Tcp,
        );
        let (ip, p) = pf.translate_dst();
        assert_eq!(ip, Ipv4Addr::new(10, 42, 0, 5));
        assert_eq!(p, 80);
    }
    #[test]
    fn t_pf_record() {
        let mut pf = PortForward::dnat(
            NetNsId(1),
            Ipv4Addr::new(192, 168, 1, 100),
            8080,
            Ipv4Addr::new(10, 42, 0, 5),
            80,
            Protocol::Tcp,
        );
        pf.record_connection(1024);
        pf.record_connection(512);
        assert_eq!(pf.connections, 2);
        assert_eq!(pf.bytes_forwarded, 1536);
    }
    #[test]
    fn t_pf_masq() {
        let pf = PortForward::masquerade(NetNsId(1), Ipv4Addr::new(10, 42, 0, 5));
        assert_eq!(pf.nat_type, NatType::Masquerade);
        assert_eq!(pf.container_ip, Ipv4Addr::new(10, 42, 0, 5));
    }
    #[test]
    fn t_cnm_create_destroy_ns() {
        let mut m = ContainerNetManager::new();
        let id = m.create_namespace("c1", 1000);
        assert_eq!(m.total_namespaces(), 1);
        assert!(m.destroy_namespace(id));
        assert_eq!(m.total_namespaces(), 0);
    }
    #[test]
    fn t_cnm_ns_has_firewall() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("t", 1);
        assert!(m.firewall_chains.contains_key(&(ns, FirewallChain::Input)));
        assert!(m.firewall_chains.contains_key(&(ns, FirewallChain::Output)));
        assert!(
            m.firewall_chains
                .contains_key(&(ns, FirewallChain::Forward))
        );
    }
    #[test]
    fn t_cnm_bridge() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("host", 0);
        let bid = m.create_bridge("br0", h);
        assert_eq!(m.total_bridges(), 1);
        assert_eq!(m.get_bridge(bid).unwrap().name, "br0");
        assert!(m.get_bridge(bid).unwrap().iface.up);
    }
    #[test]
    fn t_cnm_veth() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let c = m.create_namespace("c", 1000);
        let b = m.create_bridge("br0", h);
        let v = m.create_veth_pair(h, c, "v0", "e0", Some(b));
        assert!(v.is_ok());
        assert_eq!(m.total_veth_pairs(), 1);
        assert_eq!(m.get_namespace(c).unwrap().interfaces.len(), 2);
    }
    #[test]
    fn t_cnm_destroy_veth() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let c = m.create_namespace("c", 1000);
        let b = m.create_bridge("br0", h);
        let v = m.create_veth_pair(h, c, "v0", "e0", Some(b)).unwrap();
        assert!(m.destroy_veth_pair(v));
        assert_eq!(m.total_veth_pairs(), 0);
    }
    #[test]
    fn t_cnm_port_fwd() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        let id = m.add_port_forward(ns, 8080, Ipv4Addr::new(10, 42, 0, 5), 80, Protocol::Tcp);
        assert_eq!(m.total_port_forwards(), 1);
        assert!(m.remove_port_forward(id));
    }
    #[test]
    fn t_cnm_pf_lookup() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        m.add_port_forward(ns, 8080, Ipv4Addr::new(10, 42, 0, 5), 80, Protocol::Tcp);
        assert_eq!(
            m.lookup_port_forward(8080, Protocol::Tcp)
                .unwrap()
                .container_port,
            80
        );
    }
    #[test]
    fn t_cnm_fw_add() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        assert!(
            m.add_firewall_rule(
                FirewallRule::accept(ns, FirewallChain::Input)
                    .with_protocol(Protocol::Tcp)
                    .with_dst_port(80)
            )
        );
        assert_eq!(m.total_firewall_rules(), 1);
    }
    #[test]
    fn t_cnm_fw_remove() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        let r = FirewallRule::accept(ns, FirewallChain::Input).with_dst_port(80);
        let id = r.id;
        m.add_firewall_rule(r);
        assert!(m.remove_firewall_rule(id));
        assert_eq!(m.total_firewall_rules(), 0);
    }
    #[test]
    fn t_cnm_fw_policy() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        assert!(m.set_firewall_policy(ns, FirewallChain::Input, FirewallVerdict::Drop));
        assert_eq!(
            m.check_firewall(
                ns,
                FirewallChain::Input,
                Protocol::Tcp,
                Ipv4Addr::zero(),
                Ipv4Addr::zero(),
                None,
                None,
                None,
                None
            ),
            FirewallVerdict::Drop
        );
    }
    #[test]
    fn t_cnm_fw_check_accept() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        m.add_firewall_rule(
            FirewallRule::accept(ns, FirewallChain::Input)
                .with_protocol(Protocol::Tcp)
                .with_dst_port(443)
                .with_priority(10),
        );
        assert_eq!(
            m.check_firewall(
                ns,
                FirewallChain::Input,
                Protocol::Tcp,
                Ipv4Addr::zero(),
                Ipv4Addr::zero(),
                None,
                Some(443),
                None,
                None
            ),
            FirewallVerdict::Accept
        );
    }
    #[test]
    fn t_cnm_dns() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        m.register_dns("app.local", Ipv4Addr::new(10, 42, 0, 5), ns, 3600);
        assert_eq!(
            m.resolve_dns("app.local", ns),
            Some(Ipv4Addr::new(10, 42, 0, 5))
        );
        assert_eq!(m.total_dns_entries(), 1);
    }
    #[test]
    fn t_cnm_dns_unreg() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        let id = m.register_dns("t.local", Ipv4Addr::new(10, 42, 0, 5), ns, 3600);
        assert!(m.unregister_dns(id));
        assert_eq!(m.total_dns_entries(), 0);
        assert_eq!(m.resolve_dns("t.local", ns), None);
    }
    #[test]
    fn t_cnm_pkt_delivered() {
        let mut m = ContainerNetManager::new();
        let s = m.create_namespace("s", 1000);
        let d = m.create_namespace("d", 2000);
        assert!(matches!(
            m.simulate_packet(
                s,
                d,
                Ipv4Addr::new(10, 42, 0, 1),
                Ipv4Addr::new(10, 42, 0, 2),
                Protocol::Tcp,
                Some(12345),
                Some(80),
                1024
            ),
            PacketResult::Delivered { .. }
        ));
    }
    #[test]
    fn t_cnm_pkt_dropped() {
        let mut m = ContainerNetManager::new();
        let s = m.create_namespace("s", 1000);
        let d = m.create_namespace("d", 2000);
        m.set_firewall_policy(d, FirewallChain::Input, FirewallVerdict::Drop);
        assert!(matches!(
            m.simulate_packet(
                s,
                d,
                Ipv4Addr::new(10, 42, 0, 1),
                Ipv4Addr::new(10, 42, 0, 2),
                Protocol::Tcp,
                Some(12345),
                Some(80),
                1024
            ),
            PacketResult::Dropped(_)
        ));
    }
    #[test]
    fn t_cnm_cascade() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let c = m.create_namespace("c", 1000);
        let b = m.create_bridge("br0", h);
        m.create_veth_pair(h, c, "v0", "e0", Some(b)).unwrap();
        m.add_port_forward(c, 8080, Ipv4Addr::new(10, 42, 0, 5), 80, Protocol::Tcp);
        m.register_dns("t.local", Ipv4Addr::new(10, 42, 0, 5), c, 3600);
        assert_eq!(m.total_veth_pairs(), 1);
        assert!(m.destroy_namespace(c));
        assert_eq!(m.total_veth_pairs(), 0);
        assert_eq!(m.total_port_forwards(), 0);
        assert_eq!(m.total_dns_entries(), 0);
    }
    #[test]
    fn t_cnm_multi_container() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let b = m.create_bridge("br0", h);
        let c1 = m.create_namespace("c1", 1000);
        let c2 = m.create_namespace("c2", 1001);
        let c3 = m.create_namespace("c3", 1002);
        m.create_veth_pair(h, c1, "v0", "e0", Some(b)).unwrap();
        m.create_veth_pair(h, c2, "v1", "e0", Some(b)).unwrap();
        m.create_veth_pair(h, c3, "v2", "e0", Some(b)).unwrap();
        assert_eq!(m.total_namespaces(), 4);
        assert_eq!(m.total_veth_pairs(), 3);
        assert_eq!(m.total_bridges(), 1);
    }
    #[test]
    fn t_cnm_dhcp_alloc() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let b = m.create_bridge("br0", h);
        let c1 = m.create_namespace("c1", 1000);
        let c2 = m.create_namespace("c2", 1001);
        let v1 = m.create_veth_pair(h, c1, "v0", "e0", Some(b)).unwrap();
        let v2 = m.create_veth_pair(h, c2, "v1", "e0", Some(b)).unwrap();
        let p1 = m.veth_pairs.get(&v1).unwrap();
        let p2 = m.veth_pairs.get(&v2).unwrap();
        assert!(!p1.container_iface.ip.is_zero());
        assert!(!p2.container_iface.ip.is_zero());
        assert_ne!(p1.container_iface.ip, p2.container_iface.ip);
    }
    #[test]
    fn t_cnm_report() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let b = m.create_bridge("br0", h);
        let c = m.create_namespace("c", 1000);
        m.create_veth_pair(h, c, "v0", "e0", Some(b)).unwrap();
        m.add_port_forward(c, 8080, Ipv4Addr::new(10, 42, 0, 5), 80, Protocol::Tcp);
        let r = m.report();
        assert!(r.contains("Container Network Report"));
        assert!(r.contains("Namespaces:      2"));
        assert!(r.contains("Bridges:         1"));
    }
    #[test]
    fn t_iface_kind_str() {
        assert_eq!(InterfaceKind::Loopback.as_str(), "loopback");
        assert_eq!(InterfaceKind::Veth.as_str(), "veth");
        assert_eq!(InterfaceKind::Bridge.as_str(), "bridge");
    }
    #[test]
    fn t_proto_str() {
        assert_eq!(Protocol::Tcp.as_str(), "tcp");
        assert_eq!(Protocol::Udp.as_str(), "udp");
        assert_eq!(Protocol::Icmp.as_str(), "icmp");
        assert_eq!(Protocol::Any.as_str(), "any");
    }
    #[test]
    fn t_nat_str() {
        assert_eq!(NatType::Dnat.as_str(), "DNAT");
        assert_eq!(NatType::Snat.as_str(), "SNAT");
        assert_eq!(NatType::Masquerade.as_str(), "MASQUERADE");
    }
    #[test]
    fn t_route_default() {
        let r = Route::default(Ipv4Addr::new(10, 42, 0, 1), 1);
        assert!(r.is_default());
        assert_eq!(r.gateway, Ipv4Addr::new(10, 42, 0, 1));
        assert_eq!(r.iface_id, 1);
        assert_eq!(r.metric, 100);
    }
    #[test]
    fn t_iface_has_ip() {
        assert!(!NetInterface::new_veth("e0").has_ip());
        let mut b = NetInterface::new_bridge("br0");
        b.ip = Ipv4Addr::new(10, 42, 0, 1);
        assert!(b.has_ip());
    }
    #[test]
    fn t_ns_stats_methods() {
        let s = NetNsStats {
            total_rx_bytes: 1000,
            total_tx_bytes: 500,
            total_rx_packets: 10,
            total_tx_packets: 5,
            total_rx_errors: 1,
            ..Default::default()
        };
        assert_eq!(s.total_bytes(), 1500);
        assert_eq!(s.total_packets(), 15);
        assert!((s.error_rate() - 1.0 / 15.0).abs() < 0.001);
    }
    #[test]
    fn t_dns_default() {
        assert!(DnsConfig::default().servers.is_empty());
    }
    #[test]
    fn t_dns_new() {
        let c = DnsConfig::new(vec![Ipv4Addr::new(8, 8, 8, 8)]);
        assert_eq!(c.servers.len(), 1);
        assert_eq!(c.ndots, 1);
        assert_eq!(c.timeout_ms, 5000);
        assert_eq!(c.attempts, 3);
    }
    #[test]
    fn t_cnm_masq() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        let id = m.add_masquerade(ns, Ipv4Addr::new(10, 42, 0, 5));
        assert_eq!(
            m.port_forwards.get(&id).unwrap().nat_type,
            NatType::Masquerade
        );
    }
    #[test]
    fn t_fw_jump() {
        assert!(matches!(
            FirewallVerdict::Jump(42),
            FirewallVerdict::Jump(42)
        ));
    }
    #[test]
    fn t_bridge_dhcp_alloc() {
        let mut b = Bridge::new("br0", NetNsId(0));
        b.enable_dhcp(
            Ipv4Addr::new(10, 42, 0, 10),
            Ipv4Addr::new(10, 42, 0, 15),
            24,
        );
        let ip = b.allocate_ip(100).unwrap();
        assert!(!ip.is_zero());
        b.release_ip(ip);
        assert!(!b.dhcp_pool.as_ref().unwrap().allocated.contains_key(&ip));
    }
    #[test]
    fn t_bridge_leased_ips() {
        let mut b = Bridge::new("br0", NetNsId(0));
        b.enable_dhcp(
            Ipv4Addr::new(10, 42, 0, 10),
            Ipv4Addr::new(10, 42, 0, 15),
            24,
        );
        b.allocate_ip(100);
        b.allocate_ip(101);
        assert_eq!(b.leased_ips().len(), 2);
    }
    #[test]
    fn t_dhcp_expired() {
        let mut p = DhcpPool {
            start: Ipv4Addr::new(10, 42, 0, 10),
            end: Ipv4Addr::new(10, 42, 0, 20),
            lease_hours: 1,
            allocated: BTreeMap::new(),
            next_lease: Ipv4Addr::new(10, 42, 0, 10),
        };
        p.allocate(1);
        let ip = p.allocated.keys().next().copied().unwrap();
        assert!(!p.is_expired(ip, 100));
        if let Some(l) = p.allocated.get_mut(&ip) {
            l.expires_at = 50;
        }
        assert!(p.is_expired(ip, 100));
    }
    #[test]
    fn t_veth_peer_link() {
        let v = VethPair::new("v0", "e0", NetNsId(1), NetNsId(2));
        assert_eq!(v.host_iface.peer_iface, Some(v.container_iface.id));
        assert_eq!(v.container_iface.peer_iface, Some(v.host_iface.id));
    }
    #[test]
    fn t_subnet_mask() {
        assert_eq!(IpSubnet::mask_from_prefix(24), 0xFFFFFF00);
        assert_eq!(IpSubnet::mask_from_prefix(16), 0xFFFF0000);
        assert_eq!(IpSubnet::mask_from_prefix(32), 0xFFFFFFFF);
        assert_eq!(IpSubnet::mask_from_prefix(0), 0);
    }
    #[test]
    fn t_pf_src_translate() {
        let pf = PortForward::dnat(
            NetNsId(1),
            Ipv4Addr::new(192, 168, 1, 100),
            8080,
            Ipv4Addr::new(10, 42, 0, 5),
            80,
            Protocol::Tcp,
        );
        let (ip, p) = pf.translate_src(Ipv4Addr::new(203, 0, 113, 1));
        assert_eq!(ip, Ipv4Addr::new(203, 0, 113, 1));
        assert_eq!(p, 8080);
    }
    #[test]
    fn t_cnm_pkt_fwd_dnat() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        m.add_port_forward(ns, 8080, Ipv4Addr::new(10, 42, 0, 5), 80, Protocol::Tcp);
        let r = m.simulate_packet(
            m.namespaces.keys().next().copied().unwrap_or(NetNsId(0)),
            ns,
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(192, 168, 1, 100),
            Protocol::Tcp,
            Some(12345),
            Some(8080),
            1024,
        );
        assert!(matches!(r, PacketResult::Forwarded { .. }));
    }
    #[test]
    fn t_iface_subnet() {
        let mut i = NetInterface::new_veth("e0");
        i.ip = Ipv4Addr::new(10, 42, 0, 5);
        i.prefix_len = 24;
        assert_eq!(i.subnet(), IpSubnet::new(Ipv4Addr::new(10, 42, 0, 0), 24));
    }
    #[test]
    fn t_cnm_bridge_stats() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let b = m.create_bridge("br0", h);
        assert!(m.bridge_stats(b).is_some());
    }
    #[test]
    fn t_cnm_fw_stats() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        assert!(m.firewall_stats(ns, FirewallChain::Input).is_some());
    }
    #[test]
    fn t_cnm_pf_stats() {
        let mut m = ContainerNetManager::new();
        let ns = m.create_namespace("c", 1000);
        m.add_port_forward(ns, 8080, Ipv4Addr::new(10, 42, 0, 5), 80, Protocol::Tcp);
        assert_eq!(m.port_forward_stats().len(), 1);
    }
    #[test]
    fn t_ns_list_ifaces() {
        let ns = NetworkNamespace::new("t", 1);
        assert_eq!(ns.list_interfaces().len(), 1);
    }
    #[test]
    fn t_ns_get_iface_mut() {
        let mut ns = NetworkNamespace::new("t", 1);
        let v = NetInterface::new_veth("e0");
        let id = v.id;
        ns.add_interface(v);
        assert!(ns.get_interface_mut(id).is_some());
    }
    #[test]
    fn t_bridge_destroy() {
        let mut m = ContainerNetManager::new();
        let h = m.create_namespace("h", 0);
        let b = m.create_bridge("br0", h);
        assert!(m.destroy_bridge(b));
        assert_eq!(m.total_bridges(), 0);
    }
    #[test]
    fn t_veth_err_no_ns() {
        let mut m = ContainerNetManager::new();
        assert!(
            m.create_veth_pair(NetNsId(99), NetNsId(98), "v", "e", None)
                .is_err()
        );
    }
    #[test]
    fn t_fw_log_prefix() {
        let r = FirewallRule::new(NetNsId(1), FirewallChain::Input, FirewallVerdict::Log)
            .with_log("DROPPED");
        assert_eq!(r.log_prefix.as_deref(), Some("DROPPED"));
    }
    #[test]
    fn t_fw_comment() {
        let r = FirewallRule::accept(NetNsId(1), FirewallChain::Input).with_comment("test rule");
        assert_eq!(r.comment.as_deref(), Some("test rule"));
    }
}
