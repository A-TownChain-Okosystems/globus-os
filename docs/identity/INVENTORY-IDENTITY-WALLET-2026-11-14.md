# Bestandsaufnahme Identity & Wallet (E0)

**Dokument-ID:** GOS-IDENTITY-INVENTORY-2026-11-14
**Stand:** 14.11.2026
**Prüfgegenstand:** die Architektur-Spezifikation „KAI-OS / GlobusOS — Login,
Registrierung, Identity & Wallet Recovery" (§1–§16) gegen den Quellstand
`37bf130` (`main`) des Repositorys `A-TownChain-Okosystems/globus-os`
**Art:** reine Bestandsaufnahme (Phase E0 der Spezifikation). Es wurde nichts
geändert, nichts implementiert; die einzigen Schreibvorgänge dieses Vorgangs
betreffen Dokumente.
**Ergebnis in einem Satz:** Die funktionale Architektur trifft zu, ihre
Bestandsannahmen nicht — zwei der drei in §3.3 genannten Implementierungen
existieren in diesem Repository nicht, und der P0-Konflikt hat eine andere
Ursache als beschrieben.

---

## 1. Methode

Gelesen wurden Quelltext, Manifeste und Dokumente des Repositorys; jede Aussage
unten nennt Datei und Zeile. Es wurden keine Annahmen aus früheren Ständen
übernommen: die Zahlen in §3.3 der Spezifikation wurden einzeln nachgeprüft, auch
die, die plausibel klangen.

Der Testlauf dieses Vorgangs: `cargo +1.100.0 test --workspace` → **1824 Tests,
0 Fehler** (Rust 1.100.0, Offline). Die Vorgabe `rust-version = "1.100"` in
`Cargo.toml` ist ernst: das im Arbeitsbereich vorinstallierte Standardwerkzeug
ist 1.99.1 und lehnt den Workspace ab. Ohne `+1.100.0` sieht ein Lauf wie ein
fehlendes Werkzeug aus, obwohl nur die Fassung fehlt (Fund F-4).

---

## 2. Verantwortungsmatrix (§1.1) gegen den Bestand

| Komponente der Spezifikation | im Repository vorhanden als | Urteil |
|---|---|---|
| GlobusOS — Konto, Login, Recovery-Orchestrierung | `system/identity` (13 Module, `IdentityService`, `MultiAccountIdentityService`) | **vorhanden, ohne Schreibpfad für einen Tresor** |
| ATC-Wallet — Key-Derivation, Signaturen | **kein Crate `atc-wallet`**; `system/wallet-service` (Signieren über eine `KeyStore`-Schnittstelle) | **fehlt als Grenze**; Ableitung liegt stattdessen in `system/identity` |
| Identity Authority — Bindung, DID, Autorisierung | `IdentityBinding` (`system/identity/src/lib.rs:70`), `service_space/src/did.rs` (Ed25519) | **teilweise**: Bindung ja, DID in einem anderen Crate, keine Brücke |
| Local Security Layer — Schlüsselspeicher, Zugriffsschutz | `system/identity/src/secure_store.rs` (`EncryptedBlobStore`, `IdentityKeyProvider`) | **vorhanden**, aber kein eigener Baustein und ohne KEK/DEK-Trennung |
| ShivaCore — Isolation, IPC, Capabilities | `system/security` (`Capability`, `Right`, `Grant`, `authorize`), `system/ipc` (`IdentityOpcode`), Kernel mit `sys.identity` | **vorhanden** |
| Aurora AI — keine Identity/Wallet-Implementierung | `system/aurora-core` | **hält die Regel ein** (keine Schlüsseltypen) |

---

## 3. Die Schnittstellen aus §6 gegen den Bestand

Legende: **JA** = vorhanden und benannt · **TEILS** = vorhanden, aber mit anderer
Form oder Lücke · **NEIN** = nicht vorhanden.

### 3.1 GlobusOS Account Interface (§6.1)

| Operation der Spezifikation | Bestand | Urteil |
|---|---|---|
| `register_account` | `IdentityService::register` (`service.rs:73`), `MultiAccountIdentityService::register` (`multi_account.rs:55`), IPC `IdentityOpcode::Register = 0x0900` | **JA** |
| `authenticate_local` | `IdentityService::login` (`service.rs:186`), `IdentityOpcode::Login = 0x0901` | **JA** |
| `begin_recovery` / `complete_recovery` | ein einzelner Opcode `Recover = 0x0904` (`ipc/src/lib.rs:71`); keine Zweiteilung | **TEILS** — die Zweistufigkeit der Spezifikation fehlt im Vertrag |
| `change_password` | **nicht vorhanden**; `IdentityService` kennt `persist`/`load_record`, aber keinen Passwortwechsel | **NEIN** |
| `lock_account` | `IdentityService::lock` (`service.rs:217`), `IdentityOpcode::Lock = 0x0903` | **JA** |
| `get_account_status` | `session(now)` (`service.rs:223`), `profile()`, `binding()`; kein zusammengefasster Status | **TEILS** |

### 3.2 ATC-Wallet Interface (§6.2)

| Operation | Bestand | Urteil |
|---|---|---|
| `generate_mnemonic` | `create_wallet` → `Mnemonic::generate_in(Language::English, 24)` (`identity/src/lib.rs:237`) | **JA**, aber im Identity-Crate, nicht in einer Wallet |
| `validate_mnemonic` | `Mnemonic::parse_in_normalized` + Wortzahlprüfung (`lib.rs:269`), `RecoveryConfirmation` (`account_lifecycle.rs:58`) | **JA** |
| `derive_wallet_keys` | `derive_private_key`: `HMAC-SHA256(key = "A-TownChain", seed)` (`lib.rs:286`) | **JA — eine einzige Strecke, kein Pfad, keine Domäne** |
| `get_wallet_public_identity` | `public_key_to_address`: doppelt SHA-256, 35 Zeichen `ATC…` (`lib.rs:298`) | **JA** |
| `sign_transaction` | `WalletService::sign` (`wallet-service/src/lib.rs:91`) über `KeyStore`; in Tests ein Platzhalter, **kein echtes Signaturverfahren** | **TEILS** |
| `restore_wallet` | `restore_wallet` (`lib.rs:258`), `IdentityService::restore` (`service.rs:106`) | **JA** |

**Befund, der die Spezifikation betrifft:** Es gibt in diesem Repository keine
Wallet, die einen Schlüssel besitzt. `derive_private_key` erzeugt 32 Byte, die
*als privater Schlüssel verwendet werden*, ohne dass je ein Signaturverfahren auf
sie angewendet wird; `WalletService` signiert über eine Schnittstelle, deren
Testimplementierung eine feste Bytefolge zurückgibt
(`wallet-service/src/lib.rs:153`). Die Aussage „ATC-Wallet bleibt Eigentümer der
Wallet-Schlüssel" ist deshalb heute eine **Absichtserklärung, kein Zustand**.

### 3.3 Identity Authority Interface (§6.3)

| Operation | Bestand | Urteil |
|---|---|---|
| `create_identity` | `AccountProfile::new` (`lib.rs:206`), DID-Erzeugung in `service_space/src/did.rs` | **TEILS** (zwei Orte, keine Brücke) |
| `derive_identity_keys` | **nicht vorhanden** — es gibt genau eine Ableitung (F-1 in `SPEC-RECOVERY-001.md`) | **NEIN** |
| `verify_identity` | `IdentityBinding`-Prüfung; DID-Verifikation in `did.rs` | **TEILS** |
| `bind_wallet_identity` | `IdentityBinding` + `docs/ACCOUNT_WALLET_AURORA_NODE_BINDING.md` (spezifiziert, nicht verdrahtet) | **TEILS** |
| `restore_identity` | `IdentityService::restore` | **JA** (ohne Identitätsschlüssel, weil es keinen gibt) |
| `authorize_capability` | `security::authorize(grant, right)` (`security/src/lib.rs:48`) | **JA** |

### 3.4 Local Security Layer Interface (§6.4)

| Operation | Bestand | Urteil |
|---|---|---|
| `create_key_store` | `EncryptedBlobStore` (`secure_store.rs`), `InMemoryBlobStore`, `FileBlobStore` | **TEILS** — Blobspeicher ja, „Key Store" als Vertrag nein |
| `unlock_key_store` / `lock_key_store` | `IdentityKeyProvider`, `LoginState`, `IdentityService::lock` | **TEILS** (ohne eigenes Entsperrobjekt) |
| `rewrap_key_store` | **nicht vorhanden** | **NEIN** |
| `secure_memory_release` | `Zeroizing`, `zeroize` in `system/identity` | **JA** (auf Sprachebene) |
| `authorize_key_access` | `Right::Sign`, `Capability`, `IdentityKeyLoadRequest.capability_token` (`ipc/src/lib.rs:95`) | **JA** |

### 3.5 IPC-Sicherheitsvertrag (§6.5)

Die geforderten Felder sind im Nachrichtenkopf vorhanden (`system/ipc/src/lib.rs:22`),
mit einer Lücke: **Replay-Schutz** ist nicht Teil des Kopfs. Der Vertrag verlangt
ihn „soweit erforderlich"; die Stelle, an der er erfüllt würde (Challenge im
Bindungsverfahren), ist spezifiziert, aber nicht umgesetzt.

---

## 4. Kryptografischer Bestand (Grundlage für §3 und §4 der Spezifikation)

| Verfahren | Wo | Fassung |
|---|---|---|
| Argon2id | `system/identity-core`, `system/identity` | `argon2` 0.5 (Kern `no_std` + `alloc`) |
| XChaCha20-Poly1305 | `system/identity/src/secure_store.rs` | `chacha20poly1305` 0.10 |
| BIP39 (24 Wörter, Englisch) | `system/identity` | `bip39` |
| HMAC-SHA256, SHA-256 | `system/identity` (Ableitung, Adresse) | `hmac`, `sha2` |
| Ed25519 | `service_space` (`did.rs`, `p2p.rs`, `p2p_secure.rs`), `system/package`, `system/update` | `ed25519-dalek` 2.1 (optional) / 3.0 |
| **secp256k1** | **nur** in `modules/atc-shivacore/legacy/kernel-src-uncompiled/module_security.rs` | nicht gebaut, nicht übersetzt |
| **KEK/DEK-Trennung** | — | **nicht vorhanden** (kein Vorkommen in `system/`) |
| **CSPRNG im Image** | — | **nicht vorhanden** (kein `RDRAND`/`RDSEED` im Quelltext) |
| **Rate-Limit/Sperre** | — | **nicht vorhanden** (`system/identity`, `system/security` ohne Zähler) |

---

## 5. Korrekturen an §3.3 der Spezifikation

Die Konflikttabelle ist im Kern richtig — es gibt einen P0-Blocker —, aber drei
Zeilen beschreiben einen Bestand, den es so nicht gibt:

**K-1 · „ShivaCore Kernel: u128 und Ed25519" — beides trifft nicht zu.**
Beträge im Kettenteil sind `u64` (`service_space/src/genesis.rs:93`
`pub amount: u64`), nicht `u128`. `u128` erscheint im Kernel genau einmal, in
einer Umrechnung TSC → Millisekunden (`kernel/src/main.rs:187`) — eine
Rechenhilfe, kein Betragsformat. Ed25519 liegt im **Service-Space**
(`service_space/src/did.rs`), nicht im Kernel; der Kernel selbst kennt kein
Signaturverfahren.

**K-2 · „ATC-Wallet: u128 und secp256k1" — dieses Crate gibt es hier nicht.**
`find` nach einem Crate `atc-wallet` liefert nichts; es existiert
`system/wallet-service` (`globus-wallet-service`). Kein kompilierter Quelltext
dieses Repositorys hängt von `secp256k1` ab. Die Wallet-Ableitung liegt in
`system/identity` und benutzt HMAC-SHA256 — ein Schlüssel ohne Signaturverfahren.

**K-3 · „ATC-STD-600 v1.3.0: u64 BE, Ed25519" ist von hier aus nicht prüfbar.**
`atc-standards` ist ein eigenes Repository; diese Bestandsaufnahme kann dazu
nichts sagen. Was sie sagen kann: dieses Repository implementiert **weder** die
eine **noch** die andere Fassung vollständig, weil die Signaturstufe fehlt.

**Folge für den P0-Blocker:** Er bleibt bestehen, aber sein Gegenstand verschiebt
sich. Die Frage ist nicht „welche von drei Implementierungen hat recht", sondern:
**es gibt genau eine Ableitung und kein Signaturverfahren.** Bevor Ed25519 gegen
secp256k1 entschieden wird, muss entschieden werden, *dass* die Wallet überhaupt
signiert — und mit welchem Schlüsseltyp die 32 Byte aus `derive_private_key`
gemeint sind.

---

## 6. Was daraus für die Phasen E0–E3 folgt

| Punkt der Spezifikation | Ergebnis dieser Inventur |
|---|---|
| E0 Repository-Inventar | **erledigt durch dieses Dokument** |
| E1 Krypto-Standards bereinigen | **blockiert wie beschrieben**, nun mit zutreffender Begründung (K-1..K-3) |
| E2 Wallet-/Identity-Key-Contracts | **kann beginnen**: die Kontrakte müssen festlegen, was heute fehlt (Domänentrennung, Signaturverfahren, `key_version`) |
| E3 Local Security Layer | **kann beginnen**: Tresor nach `SPEC-RECOVERY-001.md`, KEK/DEK-Trennung, `rewrap` |
| E4/E5 Registrierung, Login | E5 läuft (Passwortprüfung im Image belegt), E4 blockiert an der Entropiequelle im Image |
| E6 Recovery | spezifiziert (`SPEC-RECOVERY-001.md`), nicht umgesetzt |
| E7/E8 Gerätewechsel, Audit | nicht beginnbar vor E1 |

---

## 7. Grenzen dieser Bestandsaufnahme

- Keine Aussage über andere Repositorys des Ökosystems (`atc-standards`,
  `a-townchain`, `atc-wallet`, falls es sie dort gibt). Diese Inventur gilt für
  `globus-os` bei `37bf130`.
- Keine Aussage über die Qualität der vorhandenen Kryptografie; gelesen wurden
  Verfahren und Aufrufstellen, nicht jede Implementierung Zeile für Zeile.
- Der Testlauf belegt, dass der Workspace baut und 1824 Tests hält. Er belegt
  **nicht** CI: die Workflows des Repositorys enden seit 03.11.2026 in
  `startup_failure` (`docs/CI_STATUS.md`). `CI_VERIFIED` bleibt damit für alles
  hier Genannte offen.
- Übertragen wurde über die Contents-API, ein Commit je Datei; es gibt keinen
  atomaren Sammel-Commit. Zwischen den Commits ist der Baum absichtlich
  konsistent (Dokumente vor dem Register).
