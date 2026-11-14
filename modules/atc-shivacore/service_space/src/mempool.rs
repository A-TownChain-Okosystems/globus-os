// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// K-Sprint 17 — Memory-Pool & Transaction-Validation
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;
/// Art einer Transaktion: Transfer (1), Stake (2), Unstake (3), Delegate (4), Vote (5), ContractCall (6), ContractDeploy (7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TxType {
    Transfer = 1,
    Stake = 2,
    Unstake = 3,
    Delegate = 4,
    Vote = 5,
    ContractCall = 6,
    ContractDeploy = 7,
}
impl TxType {
    /// Art zum Zahlenwert; unbekannte Werte ergeben None.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(TxType::Transfer),
            2 => Some(TxType::Stake),
            3 => Some(TxType::Unstake),
            4 => Some(TxType::Delegate),
            5 => Some(TxType::Vote),
            6 => Some(TxType::ContractCall),
            7 => Some(TxType::ContractDeploy),
            _ => None,
        }
    }
    /// Grundkosten der Art in Gaseinheiten, ohne Zuschlag für die Nutzlast.
    pub fn base_gas(&self) -> u64 {
        match self {
            TxType::Transfer => 1000,
            TxType::Stake => 2000,
            TxType::Unstake => 2000,
            TxType::Delegate => 1500,
            TxType::Vote => 500,
            TxType::ContractCall => 5000,
            TxType::ContractDeploy => 50000,
        }
    }
}
/// Transaktion mit Kennung, Absender, Betrag, Gasfeldern und Signatur.
#[derive(Clone, Debug, PartialEq)]
pub struct Transaction {
    /// Kennung, über alle Felder außer der Signatur berechnet.
    pub id: [u8; 32],
    /// Art der Transaktion.
    pub tx_type: TxType,
    /// Absender als DID.
    pub sender_did: String,
    /// Empfänger als DID; None bei Arten ohne Empfänger.
    pub recipient_did: Option<String>,
    /// Betrag in der kleinsten Einheit.
    pub amount: u64,
    /// Preis je Gaseinheit.
    pub gas_price: u64,
    /// Höchstes Gasbudget der Transaktion.
    pub gas_limit: u64,
    /// Fortlaufende Nummer des Absenders.
    pub nonce: u64,
    /// Zeitpunkt der Erstellung.
    pub timestamp: u64,
    /// Nutzlast der Transaktion.
    pub payload: Vec<u8>,
    /// Signatur über die Transaktion; sie geht nicht in die Kennung ein.
    pub signature: [u8; 64],
    /// Bezug auf den Proof-of-History-Eintrag.
    pub poh_hash: [u8; 32],
}
impl Transaction {
    // A full transaction spec; grouping the 11 fields into a params struct
    // is tracked design debt.
    /// Baut die Transaktion und berechnet die Kennung über alle Felder außer der Signatur.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tx_type: TxType,
        sender_did: String,
        recipient_did: Option<String>,
        amount: u64,
        gas_price: u64,
        gas_limit: u64,
        nonce: u64,
        timestamp: u64,
        payload: Vec<u8>,
        signature: [u8; 64],
        poh_hash: [u8; 32],
    ) -> Self {
        let mut i = Vec::new();
        i.push(tx_type as u8);
        i.extend_from_slice(sender_did.as_bytes());
        if let Some(ref r) = recipient_did {
            i.extend_from_slice(r.as_bytes());
        }
        i.extend_from_slice(&amount.to_be_bytes());
        i.extend_from_slice(&gas_price.to_be_bytes());
        i.extend_from_slice(&gas_limit.to_be_bytes());
        i.extend_from_slice(&nonce.to_be_bytes());
        i.extend_from_slice(&timestamp.to_be_bytes());
        i.extend_from_slice(&poh_hash);
        i.extend_from_slice(&payload);
        let id = libshivacore::simple_hash(&i);
        Transaction {
            id,
            tx_type,
            sender_did,
            recipient_did,
            amount,
            gas_price,
            gas_limit,
            nonce,
            timestamp,
            payload,
            signature,
            poh_hash,
        }
    }
    /// Grundkosten der Art plus 10 je Byte Nutzlast.
    pub fn gas_cost(&self) -> u64 {
        self.tx_type.base_gas() + (self.payload.len() as u64) * 10
    }
    /// Höchstgebühr aus Gaslimit mal Gaspreis, sättigend gerechnet.
    pub fn max_fee(&self) -> u64 {
        self.gas_limit.saturating_mul(self.gas_price)
    }
    /// Liefert die Transaktionskennung; ein eigener Nutzlast-Hash wird nicht gebildet.
    pub fn payload_hash(&self) -> [u8; 32] {
        self.id
    }
}
/// Bearbeitungsstand einer Transaktion im Pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxStatus {
    /// Angenommen, aber noch nicht geprüft.
    Pending,
    /// Geprüft und für eine Charge freigegeben.
    Validated,
    /// In den DAG übernommen.
    InDag,
    /// Bestätigt und im Zustand übernommen.
    Confirmed,
    /// Abgelehnt; der Grund steht am Eintrag.
    Rejected,
    /// Veraltet und nicht mehr ausführbar.
    Expired,
}
/// Eintrag des Pools zu einer Transaktion.
#[derive(Clone, Debug, PartialEq)]
pub struct PoolEntry {
    /// Die Transaktion selbst.
    pub tx: Transaction,
    /// Bearbeitungsstand.
    pub status: TxStatus,
    /// Zeitpunkt der Aufnahme in den Pool.
    pub added_at: u64,
    /// Zeitpunkt der Prüfung, sofern geprüft.
    pub validated_at: Option<u64>,
    /// Grund der Ablehnung, sofern abgelehnt.
    pub reject_reason: Option<String>,
    /// Rang für die Auswahl: Gaspreis mal Gaslimit.
    pub priority: u64,
}
/// Pool offener Transaktionen mit Grenzen für Anzahl und Alter.
pub struct MemoryPool {
    entries: Mutex<BTreeMap<[u8; 32], PoolEntry>>,
    by_sender: Mutex<BTreeMap<String, Vec<[u8; 32]>>>,
    max_size: usize,
    max_age_ns: u64,
}
impl MemoryPool {
    /// Pool mit Höchstzahl an Einträgen und Höchstalter in Sekunden.
    pub fn new(max_size: usize, max_age_secs: u64) -> Self {
        MemoryPool {
            entries: Mutex::new(BTreeMap::new()),
            by_sender: Mutex::new(BTreeMap::new()),
            max_size,
            max_age_ns: max_age_secs * 1_000_000_000,
        }
    }
    /// Nimmt eine Transaktion als Pending auf; voller Pool und doppelte Kennung werden abgelehnt.
    pub fn add(&self, tx: Transaction, now: u64) -> Result<(), MempoolError> {
        let mut e = self.entries.lock();
        if e.len() >= self.max_size {
            return Err(MempoolError::PoolFull);
        }
        if e.contains_key(&tx.id) {
            return Err(MempoolError::DuplicateTx);
        }
        let p = tx.gas_price.saturating_mul(tx.gas_limit);
        e.insert(
            tx.id,
            PoolEntry {
                tx: tx.clone(),
                status: TxStatus::Pending,
                added_at: now,
                validated_at: None,
                reject_reason: None,
                priority: p,
            },
        );
        self.by_sender
            .lock()
            .entry(tx.sender_did.clone())
            .or_default()
            .push(tx.id);
        Ok(())
    }
    /// Prüft Gasbudget, Empfänger und Gaspreis und setzt den Stand auf Validated; Nonce und Deckung prüft erst der TxValidator.
    pub fn validate_tx(&self, tx_id: &[u8; 32], now: u64) -> Result<(), MempoolError> {
        let mut e = self.entries.lock();
        let entry = e.get_mut(tx_id).ok_or(MempoolError::TxNotFound)?;
        if entry.tx.gas_limit < entry.tx.tx_type.base_gas() {
            entry.status = TxStatus::Rejected;
            return Err(MempoolError::GasLimitTooLow);
        }
        if entry.tx.amount > 0
            && entry.tx.recipient_did.is_none()
            && entry.tx.tx_type == TxType::Transfer
        {
            entry.status = TxStatus::Rejected;
            return Err(MempoolError::NoRecipient);
        }
        if entry.tx.gas_price == 0 {
            entry.status = TxStatus::Rejected;
            return Err(MempoolError::ZeroGasPrice);
        }
        entry.status = TxStatus::Validated;
        entry.validated_at = Some(now);
        Ok(())
    }
    /// Gibt bis zu max_count geprüfte Transaktionen zurück, nach Rang absteigend.
    pub fn get_pending_batch(&self, max_count: usize) -> Vec<Transaction> {
        let e = self.entries.lock();
        let mut v: Vec<&PoolEntry> = e
            .values()
            .filter(|x| x.status == TxStatus::Validated)
            .collect();
        v.sort_by_key(|x| core::cmp::Reverse(x.priority));
        v.into_iter()
            .take(max_count)
            .map(|x| x.tx.clone())
            .collect()
    }
    /// Setzt den Stand auf InDag, sofern die Kennung bekannt ist.
    pub fn mark_in_dag(&self, tx_id: &[u8; 32]) {
        if let Some(e) = self.entries.lock().get_mut(tx_id) {
            e.status = TxStatus::InDag;
        }
    }
    /// Setzt den Stand auf Confirmed, sofern die Kennung bekannt ist.
    pub fn mark_confirmed(&self, tx_id: &[u8; 32]) {
        if let Some(e) = self.entries.lock().get_mut(tx_id) {
            e.status = TxStatus::Confirmed;
        }
    }
    /// Entfernt bestätigte, abgelehnte und zu alte offene Einträge und liefert deren Anzahl.
    pub fn cleanup(&self, now: u64) -> usize {
        let mut e = self.entries.lock();
        let mut bs = self.by_sender.lock();
        let rm: Vec<[u8; 32]> = e
            .iter()
            .filter(|(_, x)| {
                x.status == TxStatus::Confirmed
                    || x.status == TxStatus::Rejected
                    || (x.status == TxStatus::Pending && (now - x.added_at) > self.max_age_ns)
            })
            .map(|(k, _)| *k)
            .collect();
        for t in &rm {
            if let Some(en) = e.remove(t) {
                if let Some(txs) = bs.get_mut(&en.tx.sender_did) {
                    txs.retain(|x| x != t);
                }
            }
        }
        rm.len()
    }
    /// Kopie des Eintrags, sofern vorhanden.
    pub fn get(&self, tx_id: &[u8; 32]) -> Option<PoolEntry> {
        self.entries.lock().get(tx_id).cloned()
    }
    /// Anzahl aller Einträge.
    pub fn count(&self) -> usize {
        self.entries.lock().len()
    }
    /// Anzahl der Einträge im Stand Pending.
    pub fn pending_count(&self) -> usize {
        self.entries
            .lock()
            .values()
            .filter(|e| e.status == TxStatus::Pending)
            .count()
    }
    /// Anzahl der Einträge im Stand Validated.
    pub fn validated_count(&self) -> usize {
        self.entries
            .lock()
            .values()
            .filter(|e| e.status == TxStatus::Validated)
            .count()
    }
    /// Anzahl der Einträge im Stand Confirmed.
    pub fn confirmed_count(&self) -> usize {
        self.entries
            .lock()
            .values()
            .filter(|e| e.status == TxStatus::Confirmed)
            .count()
    }
    /// Anzahl der Einträge im Stand Rejected.
    pub fn rejected_count(&self) -> usize {
        self.entries
            .lock()
            .values()
            .filter(|e| e.status == TxStatus::Rejected)
            .count()
    }
    /// Alle Transaktionen eines Absenders in der Reihenfolge ihrer Aufnahme.
    pub fn txs_by_sender(&self, did: &str) -> Vec<Transaction> {
        let bs = self.by_sender.lock();
        let e = self.entries.lock();
        bs.get(did)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| e.get(id).map(|x| x.tx.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Höchste Nonce eines Absenders im Pool; 0, wenn keine vorliegt.
    pub fn sender_nonce(&self, did: &str) -> u64 {
        let bs = self.by_sender.lock();
        let e = self.entries.lock();
        bs.get(did)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| e.get(id).map(|x| x.tx.nonce))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }
}
/// Merkt je DID die nächste erwartete Nonce.
pub struct NonceTracker {
    nonces: Mutex<BTreeMap<String, u64>>,
}
impl Default for NonceTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl NonceTracker {
    /// Leerer Zähler.
    pub fn new() -> Self {
        NonceTracker {
            nonces: Mutex::new(BTreeMap::new()),
        }
    }
    /// True und Zähler um eins weiter, wenn die Nonce passt; sonst false.
    pub fn check_and_advance(&self, did: &str, nonce: u64) -> bool {
        let mut n = self.nonces.lock();
        let e = n.entry(did.to_string()).or_insert(0);
        if nonce == *e {
            *e += 1;
            true
        } else {
            false
        }
    }
    /// Erwartete Nonce, 0 wenn die DID unbekannt ist.
    pub fn expected_nonce(&self, did: &str) -> u64 {
        self.nonces.lock().get(did).copied().unwrap_or(0)
    }
    /// Setzt die erwartete Nonce fest.
    pub fn set_nonce(&self, did: &str, nonce: u64) {
        self.nonces.lock().insert(did.to_string(), nonce);
    }
    /// Vergisst die Nonce einer DID.
    pub fn reset(&self, did: &str) {
        self.nonces.lock().remove(did);
    }
}
/// Konto mit Guthaben, Einsatz und Nonce.
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    /// Inhaber als DID.
    pub did: String,
    /// Frei verfügbares Guthaben.
    pub balance: u64,
    /// Gebundener Einsatz.
    pub staked: u64,
    /// Zuletzt gezählte Nonce.
    pub nonce: u64,
}
/// Zustandsspeicher der Konten hinter einer Sperre.
pub struct StateDb {
    accounts: Mutex<BTreeMap<String, Account>>,
}
impl Default for StateDb {
    fn default() -> Self {
        Self::new()
    }
}

impl StateDb {
    /// Leerer Speicher.
    pub fn new() -> Self {
        StateDb {
            accounts: Mutex::new(BTreeMap::new()),
        }
    }
    /// Guthaben einer DID; 0, wenn unbekannt.
    pub fn get_balance(&self, did: &str) -> u64 {
        self.accounts
            .lock()
            .get(did)
            .map(|a| a.balance)
            .unwrap_or(0)
    }
    /// Einsatz einer DID; 0, wenn unbekannt.
    pub fn get_staked(&self, did: &str) -> u64 {
        self.accounts.lock().get(did).map(|a| a.staked).unwrap_or(0)
    }
    /// Nonce einer DID; 0, wenn unbekannt.
    pub fn get_nonce(&self, did: &str) -> u64 {
        self.accounts.lock().get(did).map(|a| a.nonce).unwrap_or(0)
    }
    /// Schreibt einer DID Guthaben gut und legt sie bei Bedarf an.
    pub fn deposit(&self, did: &str, amount: u64) {
        let mut a = self.accounts.lock();
        let acc = a.entry(did.to_string()).or_insert(Account {
            did: did.to_string(),
            balance: 0,
            staked: 0,
            nonce: 0,
        });
        acc.balance += amount;
    }
    /// Bucht einen Betrag ab; zu wenig Guthaben ergibt InsufficientBalance.
    pub fn withdraw(&self, did: &str, amount: u64) -> Result<(), MempoolError> {
        let mut a = self.accounts.lock();
        let acc = a.entry(did.to_string()).or_insert(Account {
            did: did.to_string(),
            balance: 0,
            staked: 0,
            nonce: 0,
        });
        if acc.balance < amount {
            return Err(MempoolError::InsufficientBalance);
        }
        acc.balance -= amount;
        Ok(())
    }
    /// Verschiebt einen Betrag vom Guthaben in den Einsatz.
    pub fn stake(&self, did: &str, amount: u64) -> Result<(), MempoolError> {
        let mut a = self.accounts.lock();
        let acc = a.entry(did.to_string()).or_insert(Account {
            did: did.to_string(),
            balance: 0,
            staked: 0,
            nonce: 0,
        });
        if acc.balance < amount {
            return Err(MempoolError::InsufficientBalance);
        }
        acc.balance -= amount;
        acc.staked += amount;
        Ok(())
    }
    /// Verschiebt einen Betrag vom Einsatz zurück ins Guthaben.
    pub fn unstake(&self, did: &str, amount: u64) -> Result<(), MempoolError> {
        let mut a = self.accounts.lock();
        let acc = a.entry(did.to_string()).or_insert(Account {
            did: did.to_string(),
            balance: 0,
            staked: 0,
            nonce: 0,
        });
        if acc.staked < amount {
            return Err(MempoolError::InsufficientStake);
        }
        acc.staked -= amount;
        acc.balance += amount;
        Ok(())
    }
    /// Zählt die Nonce einer DID um eins weiter.
    pub fn increment_nonce(&self, did: &str) {
        let mut a = self.accounts.lock();
        let acc = a.entry(did.to_string()).or_insert(Account {
            did: did.to_string(),
            balance: 0,
            staked: 0,
            nonce: 0,
        });
        acc.nonce += 1;
    }
    /// Summe aus Guthaben und Einsatz über alle Konten.
    pub fn total_supply(&self) -> u64 {
        self.accounts
            .lock()
            .values()
            .map(|a| a.balance + a.staked)
            .sum()
    }
}
/// Prüft Transaktionen gegen Kontostand, Nonce und Mindestpreis und bucht sie ab.
pub struct TxValidator {
    state: Arc<StateDb>,
    nonces: Arc<NonceTracker>,
    min_gas_price: u64,
}
impl TxValidator {
    /// Prüfer über Zustand und Nonce-Zähler mit Mindestpreis je Gaseinheit.
    pub fn new(state: Arc<StateDb>, nonces: Arc<NonceTracker>, min_gas_price: u64) -> Self {
        TxValidator {
            state,
            nonces,
            min_gas_price,
        }
    }
    /// Prüft in fester Reihenfolge Mindestpreis, Gasbudget, Empfänger, Nonce, Deckung und Einsatz.
    pub fn validate(&self, tx: &Transaction) -> Result<(), MempoolError> {
        if tx.gas_price < self.min_gas_price {
            return Err(MempoolError::GasPriceTooLow);
        }
        if tx.gas_limit < tx.gas_cost() {
            return Err(MempoolError::GasLimitTooLow);
        }
        if tx.tx_type == TxType::Transfer && tx.amount > 0 && tx.recipient_did.is_none() {
            return Err(MempoolError::NoRecipient);
        }
        let en = self.nonces.expected_nonce(&tx.sender_did);
        if tx.nonce != en {
            return Err(MempoolError::InvalidNonce {
                expected: en,
                got: tx.nonce,
            });
        }
        if self.state.get_balance(&tx.sender_did) < tx.amount + tx.max_fee() {
            return Err(MempoolError::InsufficientBalance);
        }
        if tx.tx_type == TxType::Unstake && self.state.get_staked(&tx.sender_did) < tx.amount {
            return Err(MempoolError::InsufficientStake);
        }
        Ok(())
    }
    /// Bucht Höchstgebühr und Betrag ab, führt Einsatzwechsel aus und zählt die Nonce weiter; nicht verbrauchtes Gas wird nicht erstattet.
    pub fn apply(&self, tx: &Transaction) -> Result<(), MempoolError> {
        self.state.withdraw(&tx.sender_did, tx.max_fee())?;
        match tx.tx_type {
            TxType::Transfer => {
                let r = tx.recipient_did.as_ref().unwrap();
                self.state.withdraw(&tx.sender_did, tx.amount)?;
                self.state.deposit(r, tx.amount);
            }
            TxType::Stake => {
                self.state.stake(&tx.sender_did, tx.amount)?;
            }
            TxType::Unstake => {
                self.state.unstake(&tx.sender_did, tx.amount)?;
            }
            _ => {}
        }
        self.nonces.check_and_advance(&tx.sender_did, tx.nonce);
        self.state.increment_nonce(&tx.sender_did);
        Ok(())
    }
}
/// Ablehnungsgründe des Pools und der Prüfung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MempoolError {
    /// Der Pool hat seine Höchstzahl erreicht.
    PoolFull,
    /// Die Kennung ist bereits im Pool.
    DuplicateTx,
    /// Zur Kennung gibt es keinen Eintrag.
    TxNotFound,
    /// Das Gasbudget deckt die Grundkosten nicht.
    GasLimitTooLow,
    /// Der Gaspreis liegt unter dem Mindestpreis.
    GasPriceTooLow,
    /// Der Gaspreis ist 0.
    ZeroGasPrice,
    /// Ein Transfer mit Betrag hat keinen Empfänger.
    NoRecipient,
    /// Die Nonce weicht von der erwarteten ab.
    InvalidNonce { expected: u64, got: u64 },
    /// Das Guthaben deckt Betrag und Höchstgebühr nicht.
    InsufficientBalance,
    /// Der Einsatz reicht für das Abheben nicht.
    InsufficientStake,
    /// Die Transaktion gilt als veraltet.
    Expired,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mk(t: TxType, s: &str, r: Option<&str>, a: u64, n: u64, gp: u64, gl: u64) -> Transaction {
        Transaction::new(
            t,
            s.into(),
            r.map(|x| x.into()),
            a,
            gp,
            gl,
            n,
            1000,
            vec![],
            [0u8; 64],
            [0x42; 32],
        )
    }
    #[test]
    fn test_tx_create() {
        let t = mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000);
        assert_eq!(t.amount, 100);
    }
    #[test]
    fn test_tx_id_det() {
        assert_eq!(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000).id,
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000).id
        );
    }
    #[test]
    fn test_tx_nonce_diff() {
        assert_ne!(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000).id,
            mk(TxType::Transfer, "a", Some("b"), 100, 1, 10, 50000).id
        );
    }
    #[test]
    fn test_tx_gas_cost() {
        assert_eq!(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000).gas_cost(),
            1000
        );
    }
    #[test]
    fn test_tx_max_fee() {
        assert_eq!(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000).max_fee(),
            500000
        );
    }
    #[test]
    fn test_tx_base_gas() {
        assert_eq!(TxType::Transfer.base_gas(), 1000);
        assert_eq!(TxType::ContractDeploy.base_gas(), 50000);
    }
    #[test]
    fn test_tx_from_u8() {
        assert_eq!(TxType::from_u8(1), Some(TxType::Transfer));
        assert_eq!(TxType::from_u8(99), None);
    }
    #[test]
    fn test_pool_add() {
        let p = MemoryPool::new(100, 300);
        p.add(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000),
            1000,
        )
        .unwrap();
        assert_eq!(p.count(), 1);
    }
    #[test]
    fn test_pool_dup() {
        let p = MemoryPool::new(100, 300);
        let t = mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000);
        p.add(t.clone(), 1000).unwrap();
        assert_eq!(p.add(t, 1000), Err(MempoolError::DuplicateTx));
    }
    #[test]
    fn test_pool_full() {
        let p = MemoryPool::new(2, 300);
        p.add(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000),
            1000,
        )
        .unwrap();
        p.add(
            mk(TxType::Transfer, "a", Some("b"), 100, 1, 10, 50000),
            1000,
        )
        .unwrap();
        assert_eq!(
            p.add(
                mk(TxType::Transfer, "a", Some("b"), 100, 2, 10, 50000),
                1000
            ),
            Err(MempoolError::PoolFull)
        );
    }
    #[test]
    fn test_pool_validate() {
        let p = MemoryPool::new(100, 300);
        let t = mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000);
        let id = t.id;
        p.add(t, 1000).unwrap();
        p.validate_tx(&id, 1000).unwrap();
        assert_eq!(p.get(&id).unwrap().status, TxStatus::Validated);
    }
    #[test]
    fn test_pool_gas_low() {
        let p = MemoryPool::new(100, 300);
        let t = mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 100);
        let id = t.id;
        p.add(t, 1000).unwrap();
        assert_eq!(p.validate_tx(&id, 1000), Err(MempoolError::GasLimitTooLow));
    }
    #[test]
    fn test_pool_no_recipient() {
        let p = MemoryPool::new(100, 300);
        let t = mk(TxType::Transfer, "a", None, 100, 0, 10, 50000);
        let id = t.id;
        p.add(t, 1000).unwrap();
        assert_eq!(p.validate_tx(&id, 1000), Err(MempoolError::NoRecipient));
    }
    #[test]
    fn test_pool_zero_gas() {
        let p = MemoryPool::new(100, 300);
        let t = mk(TxType::Transfer, "a", Some("b"), 100, 0, 0, 50000);
        let id = t.id;
        p.add(t, 1000).unwrap();
        assert_eq!(p.validate_tx(&id, 1000), Err(MempoolError::ZeroGasPrice));
    }
    #[test]
    fn test_pool_batch() {
        let p = MemoryPool::new(100, 300);
        let t1 = mk(TxType::Transfer, "a", Some("b"), 100, 0, 5, 50000);
        let id1 = t1.id;
        p.add(t1, 1000).unwrap();
        let t2 = mk(TxType::Transfer, "c", Some("b"), 100, 0, 20, 50000);
        let id2 = t2.id;
        p.add(t2, 1000).unwrap();
        p.validate_tx(&id1, 1000).unwrap();
        p.validate_tx(&id2, 1000).unwrap();
        let b = p.get_pending_batch(2);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].gas_price, 20);
    }
    #[test]
    fn test_pool_confirmed() {
        let p = MemoryPool::new(100, 300);
        let t = mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000);
        let id = t.id;
        p.add(t, 1000).unwrap();
        p.mark_confirmed(&id);
        assert_eq!(p.get(&id).unwrap().status, TxStatus::Confirmed);
    }
    #[test]
    fn test_pool_cleanup() {
        let p = MemoryPool::new(100, 1);
        let t2 = mk(TxType::Transfer, "c", Some("b"), 100, 0, 10, 50000);
        let id2 = t2.id;
        p.add(t2, 1000).unwrap();
        p.mark_confirmed(&id2);
        assert_eq!(p.cleanup(2000), 1);
    }
    #[test]
    fn test_pool_by_sender() {
        let p = MemoryPool::new(100, 300);
        p.add(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000),
            1000,
        )
        .unwrap();
        p.add(
            mk(TxType::Transfer, "a", Some("b"), 100, 1, 10, 50000),
            1000,
        )
        .unwrap();
        p.add(
            mk(TxType::Transfer, "c", Some("b"), 100, 0, 10, 50000),
            1000,
        )
        .unwrap();
        assert_eq!(p.txs_by_sender("a").len(), 2);
    }
    #[test]
    fn test_pool_nonce() {
        let p = MemoryPool::new(100, 300);
        p.add(
            mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000),
            1000,
        )
        .unwrap();
        p.add(
            mk(TxType::Transfer, "a", Some("b"), 100, 2, 10, 50000),
            1000,
        )
        .unwrap();
        assert_eq!(p.sender_nonce("a"), 2);
    }
    #[test]
    fn test_nonce_seq() {
        let nt = NonceTracker::new();
        assert!(nt.check_and_advance("a", 0));
        assert!(nt.check_and_advance("a", 1));
    }
    #[test]
    fn test_nonce_skip() {
        let nt = NonceTracker::new();
        assert!(nt.check_and_advance("a", 0));
        assert!(!nt.check_and_advance("a", 5));
    }
    #[test]
    fn test_nonce_replay() {
        let nt = NonceTracker::new();
        assert!(nt.check_and_advance("a", 0));
        assert!(!nt.check_and_advance("a", 0));
    }
    #[test]
    fn test_nonce_expected() {
        let nt = NonceTracker::new();
        nt.check_and_advance("a", 0);
        nt.check_and_advance("a", 1);
        assert_eq!(nt.expected_nonce("a"), 2);
    }
    #[test]
    fn test_nonce_reset() {
        let nt = NonceTracker::new();
        nt.check_and_advance("a", 0);
        nt.reset("a");
        assert_eq!(nt.expected_nonce("a"), 0);
    }
    #[test]
    fn test_state_deposit() {
        let s = StateDb::new();
        s.deposit("a", 1000);
        assert_eq!(s.get_balance("a"), 1000);
        s.withdraw("a", 300).unwrap();
        assert_eq!(s.get_balance("a"), 700);
    }
    #[test]
    fn test_state_insufficient() {
        let s = StateDb::new();
        s.deposit("a", 100);
        assert_eq!(s.withdraw("a", 200), Err(MempoolError::InsufficientBalance));
    }
    #[test]
    fn test_state_stake() {
        let s = StateDb::new();
        s.deposit("a", 1000);
        s.stake("a", 500).unwrap();
        assert_eq!(s.get_balance("a"), 500);
        assert_eq!(s.get_staked("a"), 500);
    }
    #[test]
    fn test_state_insuf_stake() {
        let s = StateDb::new();
        s.deposit("a", 1000);
        s.stake("a", 500).unwrap();
        assert_eq!(s.unstake("a", 600), Err(MempoolError::InsufficientStake));
    }
    #[test]
    fn test_state_supply() {
        let s = StateDb::new();
        s.deposit("a", 1000);
        s.deposit("b", 500);
        s.stake("a", 200).unwrap();
        assert_eq!(s.total_supply(), 1500);
    }
    #[test]
    fn test_state_nonce_inc() {
        let s = StateDb::new();
        s.increment_nonce("a");
        s.increment_nonce("a");
        assert_eq!(s.get_nonce("a"), 2);
    }
    #[test]
    fn test_val_valid() {
        let s = Arc::new(StateDb::new());
        let n = Arc::new(NonceTracker::new());
        s.deposit("a", 1000000);
        let v = TxValidator::new(s, n, 1);
        assert!(v
            .validate(&mk(TxType::Transfer, "a", Some("b"), 100, 0, 10, 50000))
            .is_ok());
    }
    #[test]
    fn test_val_gas_low() {
        let s = Arc::new(StateDb::new());
        let n = Arc::new(NonceTracker::new());
        let v = TxValidator::new(s, n, 10);
        assert_eq!(
            v.validate(&mk(TxType::Transfer, "a", Some("b"), 100, 0, 5, 50000)),
            Err(MempoolError::GasPriceTooLow)
        );
    }
    #[test]
    fn test_val_nonce_err() {
        let s = Arc::new(StateDb::new());
        let n = Arc::new(NonceTracker::new());
        s.deposit("a", 1000000);
        let v = TxValidator::new(s, n, 1);
        assert_eq!(
            v.validate(&mk(TxType::Transfer, "a", Some("b"), 100, 5, 10, 50000)),
            Err(MempoolError::InvalidNonce {
                expected: 0,
                got: 5
            })
        );
    }
    #[test]
    fn test_val_apply_transfer() {
        let s = Arc::new(StateDb::new());
        let n = Arc::new(NonceTracker::new());
        s.deposit("a", 1000000);
        let v = TxValidator::new(s.clone(), n, 1);
        let t = mk(TxType::Transfer, "a", Some("b"), 1000, 0, 10, 50000);
        v.validate(&t).unwrap();
        v.apply(&t).unwrap();
        assert_eq!(s.get_balance("b"), 1000);
        assert_eq!(s.get_nonce("a"), 1);
    }
    #[test]
    fn test_val_apply_stake() {
        let s = Arc::new(StateDb::new());
        let n = Arc::new(NonceTracker::new());
        s.deposit("a", 1000000);
        let v = TxValidator::new(s.clone(), n, 1);
        let t = mk(TxType::Stake, "a", None, 50000, 0, 10, 50000);
        v.validate(&t).unwrap();
        v.apply(&t).unwrap();
        assert_eq!(s.get_staked("a"), 50000);
    }
    #[test]
    fn test_val_seq_nonces() {
        let s = Arc::new(StateDb::new());
        let n = Arc::new(NonceTracker::new());
        s.deposit("a", 10000000);
        let v = TxValidator::new(s.clone(), n, 1);
        for i in 0..5 {
            let t = mk(TxType::Transfer, "a", Some("b"), 100, i, 10, 50000);
            v.validate(&t).unwrap();
            v.apply(&t).unwrap();
        }
        assert_eq!(s.get_nonce("a"), 5);
    }
}
