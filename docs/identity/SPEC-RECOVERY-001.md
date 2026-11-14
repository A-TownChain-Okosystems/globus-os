# GlobusOS Recovery-Spezifikation — Identität, Wallet, Gerät

**Dokument-ID:** GOS-RECOVERY-001
**Fassung:** 0.1.0-DRAFT
**Stand:** 14.11.2026
**Eigentümer:** GlobusOS Identity Service
**Status:** SPEZIFIZIERT — keine Implementierungs- und keine Produktionsbehauptung.
Geprüft gegen den Quellstand `37bf130` (`main`).
**Vorgänger:** `docs/IDENTITY_WALLET_ARCHITECTURE.md`,
`docs/IDENTITY_WALLET_FLOW.md`, `docs/ACCOUNT_WALLET_AURORA_NODE_BINDING.md`,
`docs/LOGIN_WINDOW.md`, `docs/decisions/IDENTITY-IN-IMAGE-DECISION-2026-11-14.md`,
`docs/identity/MULTI_ACCOUNT_TRANSACTION_MODEL.md`
**Layout-Prüfer:** `scripts/check_recovery_spec.py` (rechnet die Byte-Tabellen
dieses Dokuments nach; Exit 0 = Dokument und Prüfer stimmen überein)

---

## 0. Was diese Spezifikation festlegt

Sie legt die Wiederherstellung eines GlobusOS-Kontos samt ATC-Wallet auf einem
fremden oder neu installierten Gerät fest: was die 24-Wörter-Phrase ist, was aus
ihr abgeleitet wird, welche Datensätze auf der Platte liegen, in welcher
Reihenfolge geprüft wird, wann ein Vorgang als erfolgreich gilt und wann nicht —
und wie das alles nachweisbar geprüft wird.

Sie legt **nicht** fest, wie ein ATC-Wallet-Schlüssel kryptografisch abgeleitet
und eine Adresse gebildet wird. Das bleibt bei `atc-wallet`. GlobusOS ruft es auf
und darf es nicht nachbauen (Architektur-Grundregel: eine Wahrheit je
Funktionalität).

**Vier Markierungen, die im ganzen Dokument gelten:**

| Markierung | Bedeutung |
|---|---|
| **FESTGELEGT** | Entscheidung des Owners vom 14.11.2026, bindend |
| **BEFUND** | am Quellstand nachgelesen, mit Datei/Zeile belegbar |
| **VORSCHLAG** | technische Empfehlung, braucht das Wort des Owners |
| **OFFEN** | ohne Entscheidung nicht umsetzbar |

---

## 1. Umfang der Wiederherstellung

Die offene Frage (Abschnitt 7 der Vorgabe) lautet: stellt die Phrase nur
Identität und Wallet wieder her, oder sämtliche persönlichen Daten?

**VORSCHLAG (Empfehlung, deckungsgleich mit der Vorgabe des Owners):**
Die Phrase ist die Wiederherstellungswurzel für **Identität und Wallet**, nicht
für Nutzdaten. Profildaten, Dateien und Einstellungen bekommen einen davon
getrennten Recovery-Mechanismus.

Begründung, technisch und nicht ästhetisch:

1. **Die Wurzel muss eine feste Größe haben.** Identität und Wallet sind aus der
   Phrase *deterministisch* rekonstruierbar — es genügt die Phrase, sonst nichts.
   Nutzdaten sind es nicht. Wer beides über dieselbe Wurzel zieht, macht aus
   einem 24-Wort-Backup ein Datenarchiv und muss erklären, wo die Daten liegen,
   wer sie sieht und wie groß sie sind. Die Phrase kann das nicht leisten.
2. **Der Beweischarakter bleibt trennbar.** Die Wallet ist über die Adresse
   öffentlich prüfbar; ein Konto „existiert“ also auch dann, wenn das Gerät weg
   ist. Nutzdaten haben keinen solchen Anker — sie brauchen einen eigenen
   Speicherort und eine eigene Vertraulichkeitszusage.
3. **Der Schaden bleibt begrenzt.** Wer die Phrase erbeutet, erbeutet die Wallet
   und die Identität. Er soll nicht zusätzlich automatisch alle Dokumente,
   Verläufe und Einstellungen des Nutzers bekommen.
4. **Die Datenrettung ist ein anderes Produkt.** Sie braucht Versionierung,
   Deduplizierung, Rücksetzpunkte und Löschfristen. Das in die Seed-Wurzel zu
   stecken, blockiert die Wallet-Ableitung an Anforderungen, die sie nicht hat.

**Was daraus folgt (verbindlich, sobald E-1 bestätigt ist):**

- Die Konto-Wiederherstellung stellt her: Benutzerkonto, Wallet-Adresse,
  öffentlichen Wallet-Schlüssel, Bindung zwischen beiden, neues lokales
  Anmeldepasswort, neues Geräteprofil mit **neuem** Knotenschlüssel.
- Sie stellt **nicht** her: Dateien, Anwendungseinstellungen, Verläufe,
  Sitzungsdaten, lokale Zwischenspeicher, das alte Anmeldepasswort, die
  Aurora-Knotenidentität des alten Geräts (die bleibt beim alten Gerät und wird
  bei Bedarf widerrufen, nicht kopiert).
- Ein späterer „Daten-Recovery“-Dienst bekommt einen eigenen Schlüssel und eine
  eigene Datei. Er darf **nicht** aus der Seed-Phrase abgeleitet werden; sonst
  gilt wieder Punkt 1.

---

## 2. Begriffe

| Begriff | Bedeutung |
|---|---|
| **Phrase** | 24 Wörter nach BIP39 (256 Bit Entropie + 8 Bit Prüfsumme) |
| **Entropie** | die 32 Byte Zufall, aus denen die Phrase eindeutig folgt |
| **Seed (64 Byte)** | BIP39-Seed, PBKDF2-HMAC-SHA512, 2048 Runden; nur im Speicher |
| **Tresor** | verschlüsselte Datei `GLBVAULT`, hält die Entropie und Metadaten |
| **Anmeldedatensatz** | `ATID`, 64 Byte, den der Kernel liest (Passwortprüfung) |
| **Verifier** | HKDF-Ausgang aus dem KDF-Ergebnis; belegt das richtige Passwort |
| **KEK** | Schlüssel zum Öffnen des Tresors, ebenfalls HKDF-Ausgang |
| **Geräteprofil** | lokale Installation: Konto, Tresor, Knotenschlüssel, Passwort |
| **Bindung** | Datensatz nach GOS-IDENTITY-AURORA-BINDING-001, Status `RECOVERY_REQUIRED`/`ACTIVE` |
| **key_version** | Fassung der Wallet-Ableitung; bestimmt, welche Ableitung gilt |

---

## 3. Was heute existiert und was fehlt

Diese Tabelle ist der ehrliche Teil. Alles unter „existiert“ ist am Quellstand
nachgelesen, alles unter „fehlt“ ist nicht vorhanden — auch wenn es einfach
klingt.

| Baustein | Stand | Beleg |
|---|---|---|
| Argon2id-Prüfkern, `no_std`, fail-closed | **existiert**, 9 Tests | `system/identity-core/src/lib.rs` |
| Prüfung im Kernel-Image, gemessen | **existiert**: 267 ms / 276 ms bei 19 MiB, t = 2 | `docs/decisions/IDENTITY-IN-IMAGE-DECISION-2026-11-14.md` |
| Anmeldedatensatz als Ramdisk-Datei, `sys.identity` | **existiert**, für Dienste unsichtbar | `modules/atc-shivacore/kernel/src/identity_file.rs` |
| Anmeldefenster fragt den Kernel (`0x0070`) | **existiert** | `docs/LOGIN_WINDOW.md` |
| BIP39-Erzeugung 24 Wörter, Prüfung, Ableitung | **existiert** in `system/identity` (Host, `bip39`, `Mnemonic::generate_in(English, 24)`) | `system/identity/src/lib.rs:237` |
| Wiederherstellung als Funktion | **existiert**: `IdentityService::restore(...)`, rechnet Adresse aus Phrase | `system/identity/src/service.rs:106` |
| Wortabfrage zur Bestätigung (3 Zufallspositionen) | **existiert**: `RecoveryConfirmation` | `system/identity/src/account_lifecycle.rs:58` |
| Verschlüsselter Blobspeicher (XChaCha20-Poly1305) | **existiert** | `system/identity/src/secure_store.rs` |
| Mehrkonten-Transaktion mit einer Commit-Grenze | **existiert als Modell**, nicht als Dateispeicher | `docs/identity/MULTI_ACCOUNT_TRANSACTION_MODEL.md` |
| **Tresor-Datei nach dieser Spezifikation** | **fehlt** | — |
| **Entropiequelle im Image** | **fehlt**: kein `RDRAND`/`RDSEED`, `OsRng` gibt es nur im Host-Build | `grep -r "rdrand"` leer |
| **Dauerhafter Speicher im Image** (Tresor über Neustart) | **fehlt**: Datensatz liegt auf der Ramdisk, also im Bootabbild | `identity_file.rs` |
| **Domänentrennung der Ableitung** | **fehlt**: eine Ableitung, kein Identitätsschlüssel | siehe F-1 |
| **Daten-Recovery** | **fehlt**, ausdrücklich nicht Teil dieser Fassung | Abschnitt 1 |

### Drei Befunde, die vor der Umsetzung zu entscheiden sind

**F-1 · Die Wallet-Ableitung ist eine einzige Strecke ohne Trennung.**
`derive_private_key` rechnet `HMAC-SHA256(key = b"A-TownChain", message = seed)`
(`system/identity/src/lib.rs:286`). Es gibt genau eine Ableitung. Die Vorgabe
verlangt getrennte Pfade für Wallet-Schlüssel und Identitätsschlüssel. Mit einer
Strecke ist jeder Schlüssel, der aus derselben Phrase kommt, derselbe Schlüssel —
genau das, was die Vorgabe ausschließt. Zusätzlich liegt die Ableitung in
`system/identity`, obwohl sie `atc-wallet` gehört: eine zweite Wahrheit, sobald
`atc-wallet` seine eigene mitbringt.

**F-2 · Der gespeicherte Prüfwert ist der rohe KDF-Ausgang.**
`ATID` Fassung 1 speichert `Argon2id(passwort, salt)` unverändert
(`identity-core/src/lib.rs:200`, `hash_into`). Würde man daraus den
Tresorschlüssel machen, könnte jeder, der den 64-Byte-Datensatz lesen kann, den
Tresor ohne Passwort öffnen — der Datensatz liegt offen auf der Ramdisk. Deshalb
trennt diese Spezifikation **KDF-Ausgang** (nie gespeichert) von **Verifier**
(gespeichert) und **KEK** (nie gespeichert) über zwei HKDF-Labels.

**F-3 · Zwei Passwortprüfungen in zwei Formaten.**
`system/identity/src/service.rs:236` rechnet `Argon2::default().hash_password(...)`
und legt eine PHC-Zeichenkette mit eigenem Salz ab; `identity-core` rechnet einen
rohen 32-Byte-Hash mit Salz im Datensatz. Beide benutzen zufällig dieselben
Parameter (der `argon2`-Standard ist m = 19456 KiB, t = 2, p = 1), aber es sind
zwei Formate und zwei Salze. Nach der Grundregel bleibt **eine** übrig: die des
Prüfkerns. Die PHC-Variante wird aufgelöst, nicht parallel gepflegt.

---

## 4. Die Phrase

**FESTGELEGT (aus der Vorgabe) + VORSCHLAG zur Konkretisierung:**

| Eigenschaft | Festlegung |
|---|---|
| Standard | BIP39, 24 Wörter = 256 Bit Entropie + 8 Bit Prüfsumme (264 Bit, 24 × 11) |
| Wortliste | Englisch, 2048 Wörter, fest im Programm (kein Nachladen, kein Netz) |
| Passphrase (BIP39) | **leer**, ausdrücklich und fest; v1 kennt keine BIP39-Passphrase. Ein späteres Einführen braucht eine neue `inner_version`, sonst öffnet dieselbe Phrase auf zwei Geräten zwei verschiedene Wallets |
| Normalisierung | NFKD, Kleinschreibung, ein Leerzeichen zwischen Wörtern, keine führenden/abschließenden Leerzeichen; Tabulatoren und Zeilenumbrüche gelten als Trenner |
| Erzeugung | 32 Byte aus einer CSPRNG des Systems, danach BIP39-Kodierung |
| Anzeige | einmal bei der Registrierung, danach nur nach erneuter Passwortabfrage |
| Speicherung | **niemals** im Klartext; im Tresor liegt die **Entropie** (32 Byte), nicht die Wortliste |

**BEFUND, der die Reihenfolge der Prüfung bestimmt:** Die BIP39-Prüfsumme ist
ohne KDF prüfbar. Ein falsch abgetipptes Wort fällt damit in Mikrosekunden auf,
nicht erst nach 267 ms Argon2id. Die Reihenfolge in Abschnitt 8 folgt daraus.

**VORSCHLAG zur Prüfung der Eingabe (jedes für sich ein Testfall, REC-T-05..12):**

- unbekanntes Wort → Fehler **mit Position** („Wort 7 ist kein Wort der Liste“), keine Korrektur
- 23 oder 25 Wörter → Fehler vor jeder Rechnung
- Prüfsumme falsch → Fehler vor jeder Rechnung
- doppeltes Wort an zwei Positionen → zulässig (BIP39 erlaubt das; eine Ablehnung wäre erfunden)
- Groß-/Kleinschreibung, doppelte Leerzeichen, Zeilenumbrüche → nach Normalisierung gültig
- Wortvorschläge in der Oberfläche: erst ab zwei Zeichen, nur aus der Wortliste, nie aus einer gespeicherten Phrase

---

## 5. Schlüsselhierarchie

```text
24 Wörter (BIP39, Englisch, keine Passphrase)
   |
   |  PBKDF2-HMAC-SHA512, 2048 Runden, Salz "mnemonic"
   v
Seed, 64 Byte ..................................... nur im Speicher, zeroize
   |
   +-- HKDF-SHA256, info "GLOBUS-WALLET-KEY-V1" ---> Wallet-Schlüssel (32 Byte) ---> ATC-Adresse
   |                                                                            (atc-wallet)
   +-- HKDF-SHA256, info "GLOBUS-IDENTITY-KEY-V1" --> Identitätsschlüssel -----> Konto-/Gerätebindung
   |
   +-- HKDF-SHA256, info "GLOBUS-NODE-KEY-V1" ----> Knotenschlüssel -----------> Aurora-Knoten (neues Gerät)
```

**Regeln, die daraus folgen:**

- **Trennung über `info`, nicht über Reihenfolge.** Zwei Labels ergeben zwei
  Schlüssel; niemand kann aus dem Wallet-Schlüssel den Identitätsschlüssel
  rechnen, und keiner der beiden ist der Seed.
- **Label-Längen sind geprüft, nicht geschätzt:** `GLOBUS-WALLET-KEY-V1` = 20 Byte,
  `GLOBUS-IDENTITY-KEY-V1` = 22 Byte, `GLOBUS-NODE-KEY-V1` = 18 Byte, `GLBVAULT-KEK-V1`
  = 15 Byte, `ATID-VERIFIER-V1` = 16 Byte. Jedes Label geht als `info` mit einem
  abschließenden Nullbyte ein (macht 21, 23, 19, 16, 17 Byte). Die Längen sind
  Pflichtprüfungen im Code und stehen als Kante im Test (REC-T-30).
- **Ungültige Skalare.** Ein abgeleiteter 32-Byte-Wert ist erst dann ein
  Wallet-Schlüssel, wenn er als secp256k1-Skalar gültig ist. Für den
  praktisch unmöglichen Fall ist die Regel deterministisch:
  `info = Label || 0x00 || counter` mit `counter` ab 0, erster gültiger Wert
  gewinnt. Ohne diese Regel gäbe es zwei Programme mit zwei Ergebnissen für
  denselben Fall.
- **`key_version` ist die Fassung der Ableitung, nicht des Kontos.** Fassung 1
  ist die heute belegte Strecke (`HMAC("A-TownChain")`, F-1). Fassung 2 ist die
  Hierarchie oben. Eine Wiederherstellung muss **beide** rechnen können, weil sie
  nicht weiß, mit welcher Fassung das Konto entstanden ist: die Phrase sagt
  nichts über die Ableitung.
- **Die Phrase wird beim Wiederherstellen nicht verändert.** Wer wiederherstellt,
  bekommt dieselbe Wallet-Adresse, dieselben Schlüssel und dieselbe Identität.
  Ein neues Passwort schützt den neuen Tresor, nicht einen neuen Schlüssel.

**VORSCHLAG zur Frage „welche Fassung für neue Konten“:** Fassung 2 für neue
Konten, sobald `atc-wallet` sie anbietet; Fassung 1 bleibt lesbar für bestehende
Devnet-Konten. Eine stille Umstellung bestehender Konten ist ausgeschlossen — sie
würde die Adresse ändern, und die Adresse ist die Identität.

---

## 6. Datensätze

Alle Tabellen sind mit `scripts/check_recovery_spec.py` nachgerechnet: Feld-
Versätze, Summen, Höchstlängen und die fail-closed-Fälle. Der Prüfer kennt keine
Kryptografie; er belegt, dass Dokument und Layout übereinstimmen.

### 6.1 Anmeldedatensatz `ATID` (64 Byte, unverändert in der Länge)

| Offset | Länge | Feld | Inhalt |
|---|---|---|---|
| 0 | 4 | `magic` | `ATID` |
| 4 | 1 | `format_version` | 1 = roher KDF-Ausgang (heute) · 2 = HKDF-Verifier (neu) |
| 5 | 1 | `kdf_id` | 1 = Argon2id |
| 6 | 2 | `memory_kib` | u16, 8192..32768, Vielfaches von 1024 |
| 8 | 4 | `iterations` | u32, 1..10 |
| 12 | 1 | `parallelism` | u8, 1..4 |
| 13 | 3 | `reserved` | muss null sein |
| 16 | 16 | `salt` | **dasselbe Salz wie der Tresor** |
| 32 | 32 | `verifier` | Fassung 2: `HKDF-Expand(PRK, "ATID-VERIFIER-V1\0", 32)` |

Fassung 2 ist die einzige Fassung, die mit einem Tresor zusammenarbeitet. Fassung
1 bleibt für die Anmeldung gültig, wird aber **nie** als Schlüsselquelle benutzt
(F-2). Ein Datensatz mit Fassung 1 und vorhandenem Tresor ist ein Widerspruch und
wird fail-closed abgewiesen (REC-T-22).

### 6.2 Tresor `GLBVAULT`

```text
0                     80                            80+len              96+len
+---------------------+-----------------------------+-------------------+
| Kopf, 80 Byte       | Geheimtext, len Byte        | Tag, 16 Byte      |
+---------------------+-----------------------------+-------------------+
                       |<-- AAD = die 80 Kopfbytes -->|
```

| Offset | Länge | Feld | Inhalt |
|---|---|---|---|
| 0 | 8 | `magic` | `GLBVAULT` |
| 8 | 2 | `container_version` | 1 |
| 10 | 1 | `kdf_id` | 1 = Argon2id |
| 11 | 1 | `aead_id` | 1 = XChaCha20-Poly1305 |
| 12 | 2 | `m_cost_kib` | u16, muss ≥ 19456 sein (Untergrenze, Abschnitt 10) |
| 14 | 4 | `t_cost` | u32, muss ≥ 2 sein |
| 18 | 1 | `p_cost` | u8, 1..4 |
| 19 | 1 | `flags` | muss null sein |
| 20 | 16 | `kdf_salt` | Salz für Argon2id **und** HKDF-Extract |
| 36 | 24 | `aead_nonce` | XChaCha20-Nonce, je Neuschreiben neu aus CSPRNG |
| 60 | 2 | `wallet_key_version` | Fassung der Wallet-Ableitung (Abschnitt 5) |
| 62 | 2 | `reserved` | muss null sein |
| 64 | 8 | `generation` | u64, monoton, beginnt bei 1 — Rücksetzschutz |
| 72 | 4 | `plaintext_len` | u32, 46..1024 |
| 76 | 4 | `reserved2` | muss null sein |
| 80 | `plaintext_len` | Geheimtext | XChaCha20-Poly1305 mit AAD = Kopf |
| 80+len | 16 | `tag` | Poly1305 |

**Größter Container: 80 + 1024 + 16 = 1120 Byte.** Die Obergrenze ist eine
Festlegung, keine Beobachtung: ein Tresor enthält Schlüsselmaterial und
Metadaten, keine Dateien. Wer sie erhöht, ändert die Fassung.

### 6.3 Innere Nutzlast `GLBS` (nur im Speicher, nie auf Platte)

| Offset | Länge | Feld | Inhalt |
|---|---|---|---|
| 0 | 4 | `magic` | `GLBS` |
| 4 | 2 | `inner_version` | 1 |
| 6 | 2 | `flags` | muss null sein |
| 8 | 32 | `entropy` | die 32 Byte, aus denen die 24 Wörter folgen |
| 40 | 2 | `wallet_key_version` | wie im Kopf; Abweichung = Fehler |
| 42 | 2 | `mnemonic_language` | 1 = Englisch |
| 44 | 2 | `network_len` | u16, ≤ 32 |
| 46 | `network_len` | `network` | UTF-8, z. B. `devnet` |
| … | 2 + n | `user_id` | u16-Länge + UTF-8, ≤ 128 |
| … | 2 + n | `wallet_address` | u16-Länge + UTF-8, ≤ 64 (ATC-Form: 35 Zeichen) |
| … | 2 + n | `display_name` | u16-Länge + UTF-8, ≤ 128 |

**Feste Felder: 46 Byte. Größte Nutzlast der Fassung 1: 404 Byte** (46 + 32 +
(2+128) + (2+64) + (2+128)). Nach dem letzten Feld darf **kein** Byte folgen.
Beispiel aus dem Prüfer: `devnet` / `michael` / `ATC…` / `Michael` ergibt 107 Byte
Nutzlast, 203 Byte Container.

### 6.4 Ableitung der Schlüssel aus dem Passwort

```text
EIN Argon2id-Lauf, 19 MiB, t = 2, p = 1, Ausgabe 32 Byte:
    master  = Argon2id(passwort, kdf_salt)                      <- nie gespeichert
    PRK     = HKDF-SHA256-Extract(salt = kdf_salt, ikm = master) <- nie gespeichert
    KEK     = HKDF-SHA256-Expand(PRK, "GLBVAULT-KEK-V1\0",    32)
    Verifier= HKDF-SHA256-Expand(PRK, "ATID-VERIFIER-V1\0",   32)
```

Warum genau ein Argon2id-Lauf: der Lauf kostet im Image gemessene **267 ms**
(Streuung 267..276 ms aus der Emulation ohne KVM, TSC gegen den PIT kalibrierte Referenz
2,386 GHz). Zwei Läufe wären eine halbe Sekunde vor dem ersten Bild. Ein Lauf mit
zwei HKDF-Labels kostet Mikrosekunden und trennt die beiden Zwecke trotzdem
sauber — der gespeicherte Verifier erlaubt keine Rechnung auf den KEK.

Das Salz ist **ein** Salz für Tresor und Anmeldedatensatz. Es steht im Kopf des
Tresors und im `ATID`; bei Abweichung wird nichts geöffnet (REC-T-23).

---

## 7. Zustandsdiagramm

```text
                    LOGIN  (Konto vorhanden)
                      |            \
        "Mit Seed wiederherstellen" \  Nutzername + Passwort
                      v              v
            RECOVERY_INTRO        AUTHENTICATED
                      |
                      | Gerät hat bereits ein Konto? -> OVERWRITE_CONFIRM (nur mit
                      |   bestehendem Passwort oder ausdrücklichem Neuaufsetzen)
                      v
            PHRASE_INPUT  ---  Wort unbekannt / Anzahl falsch / Prüfsumme falsch
                      |                                        |
                      | Phrase strukturell gültig               -> PHRASE_INPUT (Position nennen,
                      v                                            nichts rechnen, nichts löschen)
            DERIVE  (Seed, Wallet-Fassungen 1..n, Adresse, Bindung)
                      |
                      |  kein gültiger Skalar / Ableitung nicht verfügbar
                      |                                        -> FAILED(DERIVATION)
                      v
            OWNERSHIP_PREVIEW  (Adresse und Konto anzeigen, nichts schreiben)
                      |
                      | Abbruch                               -> ABORTED (keine Änderung)
                      v
            NEW_CREDENTIAL  (neues Passwort + Wiederholung; Regeln: Abschnitt 10)
                      |
                      | zu kurz / ungleich / leer              -> NEW_CREDENTIAL
                      v
            STAGE  (Tresor und ATID im Speicher bauen, Entropie zeroizen)
                      |
                      | KDF kann nicht rechnen (Speicher)      -> FAILED(KDF)
                      | AEAD-Fehler beim Probelauf             -> FAILED(CRYPTO)
                      v
            COMMIT  (eine Commit-Grenze, generation + 1)
                      |
                      +-- Konflikt (andere Instanz hat geschrieben) -> FAILED(CONFLICT), neu versuchen
                      +-- Schreib-/sync-Fehler                      -> FAILED(COMMIT), alter Stand gilt
                      v
            RESTORED  (Konto ACTIVE, Bindung ACTIVE, neues Geräteprofil)
                      |
                      v
            AUTHENTICATED  (Sitzung wird regulär eröffnet)
```

**Zustände, in denen nichts geschrieben ist:** `RECOVERY_INTRO`,
`PHRASE_INPUT`, `DERIVE`, `OWNERSHIP_PREVIEW`, `NEW_CREDENTIAL`. Ein Abbruch aus
diesen Zuständen hinterlässt **keine** Spur außer einem Audit-Eintrag ohne
Geheimnisse.

**Zustand, in dem es keinen halben Erfolg gibt:** `COMMIT` ist ganz oder gar
nicht (Abschnitt 8). „Tresor geschrieben, Anmeldedatensatz nicht“ ist kein
zulässiger Ausgang, sondern ein Fehler, der aufgeräumt werden muss.

---

## 8. Ablauf im Detail

Die Nummern sind bindend; jeder Schritt hat eine Zusage, die als Test existiert.

**REQ-REC-01 — Eingabe bleibt lokal.** Die Wörter gehen vom Fenster direkt an
den Identity Service, einen Sprung, ohne Zwischenspeicher, ohne IPC an Dritte,
ohne Zwischenablage (falls Einfügen angeboten wird, nur über eine ausdrückliche
Aktion und mit sofortigem Leeren der Zwischenablage). Kein Dienst außer dem
Identity Service sieht die Wörter.

**REQ-REC-02 — Struktur zuerst, Rechnung danach.** Reihenfolge je Wort: Zeichen,
Wortliste, Anzahl, Prüfsumme. Erst wenn alle vier halten, wird abgeleitet. Ein
Fehler nennt die Position und die Art, nie die erwartete Lösung.

**REQ-REC-03 — Ableitung ohne Seiteneffekt.** Ableitung und Adressberechnung
erzeugen **keine** Datei, keinen Eintrag, keine Sitzung. Die berechnete Adresse
wird angezeigt, damit der Nutzer erkennt, ob es sein Konto ist (falsche Phrase
einer fremden Wallet würde sonst still ein fremdes Konto auf diesem Gerät
anlegen).

**REQ-REC-04 — Bindung wird geprüft, nicht übernommen.** Die Bindung
(`GOS-IDENTITY-AURORA-BINDING-001`) wird mit der neu berechneten Adresse
verglichen. Weicht sie ab, wird nicht geschrieben; der Vorgang endet mit
`FAILED(BINDING_MISMATCH)` und der gemeldeten Adresse.

**REQ-REC-05 — Neues Geräteprofil, nie ein Klon.** Ein neuer Knotenschlüssel, eine
neue Knotenidentität, `binding_version` erhöht, das alte Gerät bleibt eigenständig
und wird bei Bedarf widerrufen. Eine kopierte Knotenidentität wäre ein zweiter
Knoten mit derselben Identität — ausgeschlossen.

**REQ-REC-06 — Ein Commit.** Tresor, Anmeldedatensatz, Bindung und
Kontoverzeichnis werden in **einer** Transaktion geschrieben, mit
`generation`-Prüfung (Vergleich und Schreiben atomar; bei Konflikt
`FAILED(CONFLICT)` und vollständiger Rücknahme). Erfolg wird erst nach
dauerhaftem Schreiben gemeldet.

**REQ-REC-07 — Fail-closed, immer.** Jeder Fehler endet ohne anmeldbares Konto.
Es gibt keinen Pfad, der bei halbem Erfolg „weiterläuft“.

**REQ-REC-08 — Zeroize.** Entropie, Seed, abgeleitete Schlüssel, Passwort und
Wortpuffer werden nach Gebrauch überschrieben; der Tresorklartext existiert nur
innerhalb der Commit-Funktion.

**REQ-REC-09 — Kein Geheimnis in Ausgaben.** Log, Audit, Panikmeldung,
Telemetrie, IPC an Dritte, Aurora-Kontext: keine Wörter, keine Entropie, kein
Seed, kein Passwort, kein Schlüssel. Audit enthält Ergebnis und nicht-geheime
Kennungen.

**REQ-REC-10 — Wiederherstellen ändert keine Schlüssel.** Die Wiederherstellung
liefert dieselbe Adresse. Sie tauscht das lokale Passwort, den Tresor und das
Geräteprofil. Eine Schlüsselrotation ist ein eigener, ausdrücklich gestarteter
Vorgang (Bindungsspezifikation §6) und nicht Nebenwirkung einer Wiederherstellung.

**REQ-REC-11 — Kein Rückschritt.** Ein Tresor mit älterer `generation` als der
zuletzt bestätigten wird abgewiesen. Sonst wäre eine alte Kopie ein gültiger
Tresor, und ein Angreifer könnte einen früheren Zustand „wiederherstellen“.

**REQ-REC-12 — Kein stilles Überschreiben.** Existiert auf dem Gerät bereits ein
Konto, verlangt die Wiederherstellung das bestehende Passwort oder ein
ausdrückliches Neuaufsetzen. Ein zweites Konto entsteht über den Mehrkonten-Pfad,
nicht durch Überschreiben.

---

## 9. Schnittstellen

### 9.1 Kein neuer Kernel-Syscall

Die Wiederherstellung läuft **vollständig im Userspace**. Der Kernel sieht sie
nicht: er liest weiterhin nur den 64-Byte-Anmeldedatensatz, wenn sich jemand
anmeldet.

| Nummer | Bedeutung | Status |
|---|---|---|
| `0x0050` | `Console` | belegt, bleibt ausschließlich Konsole |
| `0x0060` | `SpawnProcess` | reserviert (Owner-Entscheidung), noch nicht belegt |
| `0x0070` | `IdentityCheck` | belegt: prüft das Passwort gegen den Datensatz |

Der Grund ist keine Bequemlichkeit: die Phrase darf den Kernel nicht erreichen.
Ein Syscall mit der Phrase als Nutzlast wäre ein zweiter Ort mit Geheimnissen —
genau das, was diese Spezifikation ausschließen soll.

### 9.2 Rust-Schnittstelle (Vorschlag, Namen bindend)

```rust
/// Was der Aufrufer will, ohne dass etwas geschrieben wird.
pub struct RecoveryRequest<'a> {
    pub phrase: &'a str,          // 24 Wörter, roh; Normalisierung im Dienst
    pub display_name: &'a str,
    pub credential: &'a [u8],     // neues lokales Passwort
    pub network: &'a str,
    pub chain_id: u64,
}

/// Ergebnis der Ableitung, noch ohne Seiteneffekt.
pub struct RecoveryPreview {
    pub wallet_address: WalletAddress,
    pub wallet_key_version: u16,
    pub binding_matches: bool,
    pub account_exists: bool,
}

pub trait RecoveryService {
    /// Rechnet und prüft, schreibt nichts. Mehrfach aufrufbar.
    fn preview(&self, request: &RecoveryRequest<'_>) -> Result<RecoveryPreview, RecoveryError>;

    /// Schreibt Tresor, Datensatz, Bindung und Kontoverzeichnis in einer Transaktion.
    /// Erwartet die `generation`, die der Aufrufer in `preview` gesehen hat.
    fn commit(&mut self, request: &RecoveryRequest<'_>, expected_generation: u64)
        -> Result<RecoveryOutcome, RecoveryError>;
}

pub struct RecoveryOutcome {
    pub user_id: UserId,
    pub wallet_address: WalletAddress,
    pub generation: u64,
    pub device_profile: DeviceProfileId,   // neu, nie kopiert
}
```

Zwei Stufen, nicht eine: `preview` ist wiederholbar und harmlos, `commit` ist die
einzige Stelle mit Schreibwirkung und verlangt die gesehene `generation`. Damit
ist die Konkurrenzkontrolle Teil der Schnittstelle und nicht eine Hoffnung.

**Was mit dem bestehenden `IdentityService::restore` passiert:** es bleibt als
innere Ableitung erhalten (es rechnet heute schon Adresse aus Phrase), verliert
aber die Rolle des Schreibpfads. Acht Parameter in einer Signatur sind
Schnittstellen-Schuld; die Struktur `RecoveryRequest` löst sie auf.

### 9.3 Fehlercodes (`RecoveryError`)

| Code | Bedeutung | Wirkung |
|---|---|---|
| `PhraseUnknownWord` | Wort nicht in der Liste, mit Position | nichts geschrieben |
| `PhraseWrongLength` | nicht 24 Wörter | nichts geschrieben |
| `PhraseBadChecksum` | Prüfsumme falsch | nichts geschrieben |
| `DerivationUnavailable` | keine gültige Ableitung (Fassung, Skalar) | nichts geschrieben |
| `BindingMismatch` | Bindung passt nicht zur Adresse | nichts geschrieben |
| `OverwriteRefused` | Konto vorhanden, Bestätigung fehlt | nichts geschrieben |
| `WeakCredential` | Passwortregel verletzt | nichts geschrieben |
| `KdfUnavailable` | Speicher für die KDF reicht nicht | nichts geschrieben |
| `CryptoFailure` | Verschlüsselung/Prüfung schlug fehl | nichts geschrieben |
| `GenerationConflict` | andere Instanz hat geschrieben | nichts geschrieben, Aufruf wiederholbar |
| `CommitFailed` | Schreiben/Sync fehlgeschlagen | alter Stand gilt, Aufräumen nötig |
| `VaultCorrupt` | Tresor strukturell ungültig oder manipuliert | nichts geöffnet |

---

## 10. Grenzwerte und ihre Begründung

Jeder Wert braucht eine eigene Begründung; aus den Grenzen der VM oder des
Kernels wird nichts abgeleitet.

| Wert | Festlegung | Begründung |
|---|---|---|
| Argon2id Speicher | 19 MiB (19456 KiB), Untergrenze beim Öffnen | Owner-Entscheidung 14.11.; passt in den KDF-Bereich des Images (20 MiB aus dem Frame-Vorrat) |
| Argon2id Durchgänge | 2, Untergrenze beim Öffnen | Owner-Entscheidung; gemessen 267 ms im Image |
| Parallelität | 1 (1..4 zulässig) | einkernige Emulation ist der Bezug; höhere Werte ändern die Dauer, nicht die Sicherheit |
| Salz | 16 Byte aus CSPRNG | Kollisionsfreiheit je Ausstellung; 128 Bit genügen gegen Wiederholung |
| Nonce | 24 Byte, je Schreiben neu | XChaCha20 mit Zufallsnonce ist ohne Zähler sicher; 2^192 Raum |
| Nutzlast | 46..1024 Byte | größte Fassung-1-Nutzlast ist 404 Byte; 1024 lässt Fassung 2 Luft, ohne ein Datenarchiv zu werden |
| Wörter | genau 24 | Owner-Vorgabe; 12 wird abgewiesen, nicht „unterstützt“ |
| Netzwerkname | ≤ 32 Byte | feste Kettennamen sind kurz; die Grenze verhindert aufgeblähte Tresore |
| Anzeigename | ≤ 128 Byte | wie `UserId`-Grenze, ein Wert statt zwei |
| Generation | u64, Beginn 1 | Rücksetzschutz ohne Uhrabhängigkeit |
| Wiederholversuche Passwort | keine Obergrenze beim Wiederherstellen | die Phrase ist der Nachweis, nicht das Passwort; eine Sperre würde den rechtmäßigen Nutzer aussperren |

---

## 11. Bedrohungsmodell

| Bedrohung | Wirkung ohne Maßnahme | Maßnahme in dieser Spezifikation |
|---|---|---|
| Gerät gestohlen, Tresor kopiert | Offline-Angriff auf das Passwort | Argon2id 19 MiB, t = 2; Salz je Ausstellung; Passwortregel |
| Datensatz gelesen (Ramdisk offen) | Tresor ohne Passwort | KDF-Ausgang ≠ Verifier ≠ KEK (F-2, Abschnitt 6.4) |
| Alter Tresor zurückgespielt | früherer Zustand „gültig“ | `generation` monoton, Rückschritt abgewiesen (REQ-REC-11) |
| Tresor manipuliert | stiller Betrug an Adresse/Konto | AAD über den Kopf, Tag-Prüfung, Adresse wird nach dem Öffnen neu gerechnet und verglichen |
| Phrase abgetippt/abfotografiert | Wallet weg | Anzeige einmal, danach Passwortabfrage; keine Zwischenablage, kein Bildschirmexport; Warnhinweis Pflicht |
| Zweites Gerät mit derselben Phrase | zwei Knoten, eine Identität | neues Geräteprofil, neuer Knotenschlüssel, altes Gerät widerrufbar (REQ-REC-05) |
| Wiederherstellung auf fremdes Konto | Angreifer pflanzt sein Konto ein | Adresse wird vor dem Schreiben angezeigt, Überschreiben nur mit bestehendem Passwort (REQ-REC-12) |
| Fehler mitten im Schreiben | halbes Konto, nicht anmeldbar | eine Commit-Grenze, Rücknahme (REQ-REC-06) |
| Geheimnis in Log/Panik/Telemetrie | Weitergabe ohne Angriff | REQ-REC-09 plus Kanarienvogel-Test (REC-T-19) |
| Gefälschte „Support“-Aufforderung | Nutzer gibt Phrase heraus | Programm fragt nie außerhalb des gestarteten Wiederherstellungsfensters; Text im Fenster sagt das |
| Schwache Entropie im Image | vorhersehbare Phrasen | CSPRNG-Pflicht; heute **fehlt** die Quelle im Image (Abschnitt 3) — harter Blocker für jede Seed-Erzeugung im Image |

---

## 12. Sicherheitstests

Jeder Test hat ein eindeutiges Ergebnis. „Läuft durch“ ist kein Nachweis; die
Spalte „erwartet“ ist die Zusage.

| ID | Fall | erwartet |
|---|---|---|
| REC-T-01 | gültige Phrase, richtiges neues Passwort | `RESTORED`, dieselbe Adresse wie bei der Registrierung |
| REC-T-02 | dieselbe Phrase zweimal wiederhergestellt | gleiche Adresse, gleiche Schlüssel, neue `generation` |
| REC-T-03 | Phrase eines anderen Kontos | `BindingMismatch`, nichts geschrieben |
| REC-T-04 | leere Eingabe | `PhraseWrongLength`, keine Rechnung |
| REC-T-05 | 12 Wörter | `PhraseWrongLength` |
| REC-T-06 | 23 Wörter, sonst korrekt | `PhraseWrongLength` |
| REC-T-07 | 25 Wörter | `PhraseWrongLength` |
| REC-T-08 | ein Wort ersetzt (Prüfsumme falsch) | `PhraseBadChecksum`, keine Rechnung |
| REC-T-09 | ein Wort nicht in der Liste | `PhraseUnknownWord` **mit Position** |
| REC-T-10 | Großschreibung, doppelte Leerzeichen, Zeilenumbrüche | gültig, gleiches Ergebnis wie normalisiert |
| REC-T-11 | doppeltes Wort an zwei Positionen, Prüfsumme gültig | gültig (BIP39 erlaubt es) |
| REC-T-12 | Wortvorschlag bei einem Zeichen | keine Vorschläge |
| REC-T-13 | Abbruch in `OWNERSHIP_PREVIEW` | keine Datei, keine Änderung |
| REC-T-14 | Überschreiben bei vorhandenem Konto ohne Passwort | `OverwriteRefused` |
| REC-T-15 | Passwort zu kurz | `WeakCredential`, keine Ableitung |
| REC-T-16 | KDF-Speicher nicht verfügbar (Bereich gesperrt) | `KdfUnavailable`, nichts geschrieben |
| REC-T-17 | Abbruch zwischen Staging und Commit (Crash-Injektion) | alter Stand vollständig gültig |
| REC-T-18 | zwei Wiederherstellungen gleichzeitig | eine gewinnt, die andere `GenerationConflict` |
| REC-T-19 | Wortkanarienvogel im Log/Audit/Paniktext suchen | nicht gefunden, in keinem Feld |
| REC-T-20 | Tresor-Kopf verändert (1 Bit in `m_cost_kib`) | `VaultCorrupt` durch AAD/Tag, kein Öffnen |
| REC-T-21 | Geheimtext verändert | `VaultCorrupt` |
| REC-T-22 | Anmeldedatensatz Fassung 1 mit vorhandenem Tresor | `VaultCorrupt` (Widerspruch), keine Anmeldung |
| REC-T-23 | Salz in `ATID` ≠ Salz im Tresor | `VaultCorrupt` |
| REC-T-24 | `generation` älter als bestätigt | Rückschritt abgewiesen |
| REC-T-25 | Nutzlast mit angehängten Bytes | `VaultCorrupt` (keine Toleranz) |
| REC-T-26 | Wallet-Fassung 1 und 2 aus derselben Phrase | zwei verschiedene Adressen, beide stabil |
| REC-T-27 | Adresse nach dem Öffnen nachgerechnet ≠ gespeicherte Adresse | Integritätsfehler, kein Login |
| REC-T-28 | Zeroize: Puffer nach `commit` geprüft | Entropie, Seed, Passwort, Klartext sind überschrieben |
| REC-T-29 | Wiederherstellung auf Gerät mit anderem Konto | neues Profil nur mit Bestätigung, altes unverändert |
| REC-T-30 | Label-Längen (`GLOBUS-WALLET-KEY-V1` = 20 usw.) | Übersetzungsprüfung schlägt bei Abweichung fehl |
| REC-T-31 | zwei Konten, gleiche Anzeigenamen, verschiedene Phrasen | getrennte `user_id`, getrennte Tresore |
| REC-T-32 | Tresor mit `m_cost_kib` unter 19456 | abgewiesen (keine Absenkung der Parameter) |
| REC-T-33 | Layout-Prüfer `scripts/check_recovery_spec.py` | Exit 0 gegen das Dokument |

Die Fälle 17, 18, 20, 21, 24 und 27 sind nur mit echter Implementierung prüfbar;
sie stehen hier als Abnahmebedingung, nicht als erledigt.

---

## 13. Reihenfolge der Umsetzung

Jede Stufe hat ein Tor; ohne Tor keine nächste Stufe. Nichts davon ist heute
umgesetzt.

| Stufe | Inhalt | Tor |
|---|---|---|
| **R-1** | Entropiequelle im Image (`RDRAND`/`RDSEED` mit Selbsttest, Ausfall = fail-closed) | Seed-Erzeugung im Image überhaupt möglich; Test: Ausfall wird erkannt, nicht „irgendwie“ ersetzt |
| **R-2** | Tresor-Datei nach Abschnitt 6.2/6.3, Öffnen/Schreiben mit AEAD, `generation` | REC-T-20..25, 28, 32 grün; Layout-Prüfer bleibt grün |
| **R-3** | `ATID` Fassung 2 mit Verifier/KEK-Trennung, ein Argon2id-Lauf | REC-T-22, 23; bestehende Anmeldung (Fassung 1) unverändert grün |
| **R-4** | Wiederherstellung im Identity Service: `preview`/`commit`, Bindung, Geräteprofil | REC-T-01..19, 26, 27, 29, 31 |
| **R-5** | Fenster: Phraseneingabe, Positionsfehler, Adressanzeige, neues Passwort | Bedienung im laufenden Image belegt (Bildschirmfotos + Log ohne Geheimnisse) |

Die Stufe **R-1** steht vorn, weil ohne sie jede erzeugte Phrase ein Zufall ohne
Zufall wäre. Bis dahin darf im Image **keine** Phrase erzeugt werden.

---

## 14. Offene Entscheidungen

| Nr. | Frage | Empfehlung | blockiert |
|---|---|---|---|
| **E-1** | Phrase stellt nur Identität und Wallet her, oder auch Nutzdaten? | nur Identität und Wallet; Daten eigener Mechanismus (Abschnitt 1) | R-2, R-4 |
| **E-2** | Wortliste Englisch (BIP39-Standard) oder zusätzlich Deutsch? | Englisch; eine zweite Wortliste ist eine zweite Fassung und braucht `inner_version` 2 | R-2 |
| **E-3** | `key_version` für neue Konten | 2 (getrennte Pfade), Fassung 1 bleibt lesbar | Abschnitt 5, R-4 |
| **E-4** | Wo liegt der Tresor, solange das Image keinen dauerhaften Speicher hat? | bis zum Vorhandensein: Ramdisk und ausdrücklich „nicht dauerhaft“ in der Oberfläche; kein Versprechen, das die Technik nicht hält | R-2, R-5 |
| **E-5** | Darf die Wiederherstellung eine Schlüsselrotation anbieten? | nein, getrennter Vorgang nach der Bindungsspezifikation §6 | REQ-REC-10 |

---

## 15. Nicht-Ziele

- Keine Aussage über die Sicherheit des ATC-Wallet-Verfahrens; das ist
  `atc-wallet` und die ATC-Standards.
- Keine Wiederherstellung von Dateien, Einstellungen, Verläufen oder
  Aurora-Knotenidentitäten (Abschnitt 1).
- Keine Server-, Cloud- oder Escrow-Wiederherstellung. Es gibt keinen Dienst, der
  eine Phrase kennt.
- Keine Wiederherstellung von Kettenzustand: Guthaben, Verlauf und Verträge
  liegen auf der Kette, nicht auf dem Gerät.
- Keine Aussage über Produktionsreife. `CI_VERIFIED` bleibt offen, solange die
  Workflows im Repository nicht laufen.

---

## 16. Referenzen

- `docs/IDENTITY_WALLET_ARCHITECTURE.md` — Architektur und Datenklassifikation
- `docs/IDENTITY_WALLET_FLOW.md` — Zustandsmaschine Boot → Login → Register
- `docs/ACCOUNT_WALLET_AURORA_NODE_BINDING.md` — Bindung, Rotation, `RECOVERY_REQUIRED`
- `docs/identity/MULTI_ACCOUNT_TRANSACTION_MODEL.md` — eine Commit-Grenze
- `docs/decisions/IDENTITY-IN-IMAGE-DECISION-2026-11-14.md` — KDF-Parameter und Messung im Image
- `docs/LOGIN_WINDOW.md` — Fenster, `0x0070`, Rechte an der Konsole
- `scripts/check_recovery_spec.py` — rechnet die Byte-Tabellen dieses Dokuments nach
