#!/usr/bin/env python3
"""Pruefer fuer die Byte-Layouts der Wiederherstellungsspezifikation.

Dieses Skript rechnet die Tabellen in `docs/identity/SPEC-RECOVERY-001.md`
nach, statt sie zu glauben. Es prueft:

  * die Feldversaetze des Tresor-Containers (`GLBVAULT`) und der inneren
    Nutzlast (`GLBS`) gegen die im Dokument genannten Werte,
  * dass ein gebauter Container sich mit unabhaengig geschriebenen Versaetzen
    wieder lesen laesst (Roundtrip),
  * die Grenzen (Laengenfelder, Obergrenzen, reservierte Felder),
  * die fail-closed-Faelle, die rein strukturell entscheidbar sind
    (Kennung, Fassung, Laenge, reservierte Bytes, unbekannte Sprache).

NICHT geprueft wird hier Kryptografie. Der Pruefer kennt weder Argon2id noch
XChaCha20-Poly1305; dass ein falsches Passwort den Tresor nicht oeffnet, ist
eine Aufgabe der Implementierungstests (REC-T-xx im Dokument).

Exit: 0 = alle Zusagen halten, 1 = Widerspruch (mit Stelle im Dokument).
"""

import os
import struct
import hmac
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

FEHLER = []


def pruefe(bedingung, text):
    if bedingung:
        print(f"  ok   {text}")
    else:
        print(f"  FEHL {text}")
        FEHLER.append(text)


# --------------------------------------------------------------------------
# Tresor-Container: GLBVAULT
# --------------------------------------------------------------------------

VAULT_MAGIC = b"GLBVAULT"
VAULT_HEADER_LEN = 80
VAULT_TAG_LEN = 16
VAULT_VERSION = 1
KDF_ARGON2ID = 1
AEAD_XCHACHA20_POLY1305 = 1

VAULT_MIN_PLAINTEXT = 46          # feste innere Felder, siehe unten
VAULT_MAX_PLAINTEXT = 1024        # Obergrenze, nicht "was gerade passt"
VAULT_MAX_BYTES = VAULT_HEADER_LEN + VAULT_MAX_PLAINTEXT + VAULT_TAG_LEN

# (Name, Offset, Laenge) — bindend, in dieser Reihenfolge geprueft
VAULT_FELDER = [
    ("magic", 0, 8),
    ("container_version", 8, 2),
    ("kdf_id", 10, 1),
    ("aead_id", 11, 1),
    ("m_cost_kib", 12, 2),
    ("t_cost", 14, 4),
    ("p_cost", 18, 1),
    ("flags", 19, 1),
    ("kdf_salt", 20, 16),
    ("aead_nonce", 36, 24),
    ("wallet_key_version", 60, 2),
    ("reserved", 62, 2),
    ("generation", 64, 8),
    ("plaintext_len", 72, 4),
    ("reserved2", 76, 4),
]


def vault_header(m_cost_kib, t_cost, p_cost, salt, nonce, key_version,
                 generation, plaintext_len):
    return struct.pack(
        "<8sHBBHI B B 16s 24s H H Q I I",
        VAULT_MAGIC, VAULT_VERSION, KDF_ARGON2ID, AEAD_XCHACHA20_POLY1305,
        m_cost_kib, t_cost, p_cost, 0, salt, nonce, key_version, 0,
        generation, plaintext_len, 0,
    )


def parse_vault_header(roh):
    if len(roh) != VAULT_HEADER_LEN:
        return None, "HEADER_LEN"
    (magic, version, kdf, aead, m, t, p, flags, salt, nonce, kv, reserved,
     generation, plen, reserved2) = struct.unpack(
        "<8sHBBHI B B 16s 24s H H Q I I", roh)
    if magic != VAULT_MAGIC:
        return None, "MAGIC"
    if version != VAULT_VERSION:
        return None, "VERSION"
    if kdf != KDF_ARGON2ID:
        return None, "KDF"
    if aead != AEAD_XCHACHA20_POLY1305:
        return None, "AEAD"
    if flags != 0 or reserved != 0 or reserved2 != 0:
        return None, "RESERVED"
    if not (VAULT_MIN_PLAINTEXT <= plen <= VAULT_MAX_PLAINTEXT):
        return None, "PLAINTEXT_LEN"
    if generation == 0:
        return None, "GENERATION"
    return dict(m_cost_kib=m, t_cost=t, p_cost=p, salt=salt, nonce=nonce,
                key_version=kv, generation=generation,
                plaintext_len=plen), None


# --------------------------------------------------------------------------
# Innere Nutzlast: GLBS (Klartext, nur im Speicher, nie auf Platte)
# --------------------------------------------------------------------------

INNER_MAGIC = b"GLBS"
INNER_VERSION = 1
INNER_FIXED_LEN = 46
ENTROPY_LEN = 32
SPRACHE_ENGLISCH = 1

MAX_NETWORK_LEN = 32
MAX_USER_ID_LEN = 128
MAX_ADDRESS_LEN = 64      # ATC-Adresse ist 35 Zeichen, Luft fuer spaetere Formate
MAX_DISPLAY_NAME_LEN = 128

INNER_FELDER = [
    ("magic", 0, 4),
    ("inner_version", 4, 2),
    ("flags", 6, 2),
    ("entropy", 8, 32),
    ("wallet_key_version", 40, 2),
    ("mnemonic_language", 42, 2),
    ("network_len", 44, 2),
]


def inner_pack(entropy, key_version, network, user_id, address, display_name):
    for name, wert, grenze in (
        ("network", network, MAX_NETWORK_LEN),
        ("user_id", user_id, MAX_USER_ID_LEN),
        ("address", address, MAX_ADDRESS_LEN),
        ("display_name", display_name, MAX_DISPLAY_NAME_LEN),
    ):
        if len(wert.encode("utf-8")) > grenze:
            raise ValueError(f"{name} zu lang")
    if len(entropy) != ENTROPY_LEN:
        raise ValueError("entropy muss 32 Byte sein")
    net = network.encode("utf-8")
    teile = [
        struct.pack("<4sHH", INNER_MAGIC, INNER_VERSION, 0),
        entropy,
        struct.pack("<HH H", key_version, SPRACHE_ENGLISCH, len(net)),
        net,
    ]
    for text in (user_id, address, display_name):
        roh = text.encode("utf-8")
        teile.append(struct.pack("<H", len(roh)))
        teile.append(roh)
    return b"".join(teile)


def inner_parse(roh):
    if len(roh) < INNER_FIXED_LEN:
        return None, "MIN_LEN"
    magic, version, flags = struct.unpack("<4sHH", roh[0:8])
    if magic != INNER_MAGIC:
        return None, "MAGIC"
    if version != INNER_VERSION:
        return None, "VERSION"
    if flags != 0:
        return None, "FLAGS"
    entropy = roh[8:40]
    key_version, sprache, net_len = struct.unpack("<HHH", roh[40:46])
    if sprache != SPRACHE_ENGLISCH:
        return None, "SPRACHE"
    if net_len > MAX_NETWORK_LEN:
        return None, "NETWORK_LEN"
    pos = INNER_FIXED_LEN
    if pos + net_len > len(roh):
        return None, "NETWORK_TRUNCATED"
    network = roh[pos:pos + net_len].decode("utf-8")
    pos += net_len
    texte = []
    for name, grenze in (("user_id", MAX_USER_ID_LEN),
                         ("address", MAX_ADDRESS_LEN),
                         ("display_name", MAX_DISPLAY_NAME_LEN)):
        if pos + 2 > len(roh):
            return None, f"{name.upper()}_LEN_TRUNCATED"
        (laenge,) = struct.unpack("<H", roh[pos:pos + 2])
        pos += 2
        if laenge > grenze:
            return None, f"{name.upper()}_TOO_LONG"
        if pos + laenge > len(roh):
            return None, f"{name.upper()}_TRUNCATED"
        texte.append(roh[pos:pos + laenge].decode("utf-8"))
        pos += laenge
    if pos != len(roh):
        return None, "TRAILING_BYTES"
    return dict(entropy=entropy, key_version=key_version, network=network,
                user_id=texte[0], address=texte[1],
                display_name=texte[2]), None


# --------------------------------------------------------------------------
# Anmeldedatensatz: ATID (64 Byte, Fassung 1 und 2)
# --------------------------------------------------------------------------

ATID_MAGIC = b"ATID"
ATID_LEN = 64
ATID_FELDER = [
    ("magic", 0, 4),
    ("format_version", 4, 1),
    ("kdf_id", 5, 1),
    ("memory_kib", 6, 2),
    ("iterations", 8, 4),
    ("parallelism", 12, 1),
    ("reserved", 13, 3),
    ("salt", 16, 16),
    ("verifier", 32, 32),
]



# --------------------------------------------------------------------------
# Identifikatoren und Laengen (E1.3/E1.4, Abschnitte 15.3 bis 15.5)
# --------------------------------------------------------------------------
#
# Bei Byteangaben ist Schaetzen der teure Fehler: eine um ein Zeichen falsch
# gezaehlte Domaene ergibt einen anderen Digest und damit eine andere Adresse.
# Deshalb werden hier alle Laengen aus den Zeichenketten selbst gerechnet und
# gegen die im Dokument genannten Werte gehalten.

import hashlib

DID_PREFIX = "did:shivacore:ed25519:"
ADDR_V1_PREFIX = "ATC"
ADDR_V2_PREFIX = "ATC2"
DOMAENE_ADRESSE_V2 = "ATC-ADDR-V2"
DOMAENE_ADRESSE_V2_CHECK = "ATC-ADDR-V2-CHECK"
LABEL_WALLET = "GLOBUS-WALLET-KEY-V2"
LABEL_IDENTITAET = "GLOBUS-IDENTITY-KEY-V2"
LABEL_KNOTEN = "GLOBUS-NODE-KEY-V2"

HEX_PRO_WEITE = 2          # zwei Zeichen je Byte
ADRESSE_HEX_BYTES = 14     # 14 Byte Hauptteil
ADRESSE_CHECK_BYTES = 2    # 2 Byte Pruefsumme
PUBKEY_BYTES = 32
SEED_BYTES = 32
SIGNATUR_BYTES = 64
BIP39_SEED_BYTES = 64


def adresse_v2(pubkey: bytes) -> str:
    """Rechnet die Adresse der Fassung 2 nach Abschnitt 15.5."""
    h = hashlib.sha256(DOMAENE_ADRESSE_V2.encode() + b"\x00" + pubkey).digest()
    c = hashlib.sha256(DOMAENE_ADRESSE_V2_CHECK.encode() + b"\x00" + h).digest()
    return (ADDR_V2_PREFIX
            + h[:ADRESSE_HEX_BYTES].hex().upper()
            + c[:ADRESSE_CHECK_BYTES].hex().upper())


def adresse_v1(privater_wert: bytes) -> str:
    """Rechnet die Adresse der Fassung 1 nach system/identity/src/lib.rs:298."""
    public = hashlib.sha256(privater_wert).digest()
    step1 = hashlib.sha256(public).digest()
    step2 = hashlib.sha256(step1).digest()
    checksum = hashlib.sha256(step2).digest()
    return (ADDR_V1_PREFIX
            + step2[:ADRESSE_HEX_BYTES].hex().upper()
            + checksum[:ADRESSE_CHECK_BYTES].hex().upper())


def did(pubkey: bytes) -> str:
    """Rechnet die DID nach Abschnitt 15.5 (Kleinhex, wie in did.rs)."""
    return DID_PREFIX + pubkey.hex()


def pruefe_identifikatoren() -> None:
    print("\n== Identifikatoren: Adresse v1, Adresse v2, DID (E1.3/E1.4) ==")

    pruefe(len(DID_PREFIX) == 22,
           f"DID-Praefix ist 22 Zeichen (war {len(DID_PREFIX)})")
    pruefe(len(DID_PREFIX) + 2 * PUBKEY_BYTES == 86,
           "DID ist 86 Zeichen (22 + 64)")
    pruefe(len(ADDR_V1_PREFIX) + 2 * (ADRESSE_HEX_BYTES + ADRESSE_CHECK_BYTES) == 35,
           "Adresse v1 ist 35 Zeichen")
    pruefe(len(ADDR_V2_PREFIX) + 2 * (ADRESSE_HEX_BYTES + ADRESSE_CHECK_BYTES) == 36,
           "Adresse v2 ist 36 Zeichen")
    pruefe(len(DOMAENE_ADRESSE_V2) + 1 + PUBKEY_BYTES == 44,
           f"Hash-Eingabe der Adresse v2 ist 44 Byte (Domaene {len(DOMAENE_ADRESSE_V2)} + 1 + 32)")
    pruefe(len(DOMAENE_ADRESSE_V2_CHECK) + 1 + PUBKEY_BYTES == 50,
           f"Hash-Eingabe der Pruefsumme ist 50 Byte (Domaene {len(DOMAENE_ADRESSE_V2_CHECK)} + 1 + 32)")
    pruefe(len(LABEL_WALLET) == 20, f"HKDF-Label Wallet ist 20 Byte (war {len(LABEL_WALLET)})")
    pruefe(len(LABEL_IDENTITAET) == 22,
           f"HKDF-Label Identitaet ist 22 Byte (war {len(LABEL_IDENTITAET)})")
    pruefe(len(LABEL_KNOTEN) == 18,
           f"HKDF-Label Knoten ist 18 Byte (war {len(LABEL_KNOTEN)})")

    # Ein Beispielpubkey: keine Kurve, nur 32 Byte mit aufsteigenden Werten.
    pubkey = bytes(range(PUBKEY_BYTES))
    a1 = adresse_v1(b"ein-beispielgeheimnis")
    a2 = adresse_v2(pubkey)
    d = did(pubkey)

    pruefe(len(a1) == 35, f"Adresse v1 in der Praxis 35 Zeichen (war {len(a1)})")
    pruefe(len(a2) == 36, f"Adresse v2 in der Praxis 36 Zeichen (war {len(a2)})")
    pruefe(a2.startswith(ADDR_V2_PREFIX), "Adresse v2 traegt das Praefix ATC2")
    pruefe(a2[4:] == a2[4:].upper(), "Adresse v2 ist Grosshex")
    pruefe(len(d) == 86, f"DID in der Praxis 86 Zeichen (war {len(d)})")
    pruefe(d[22:] == d[22:].lower(), "DID ist Kleinhex")
    pruefe(len(a1) != len(a2) and not a2.endswith(a1[3:]), 
           "v1- und v2-Adresse sind nicht ineinander lesbar")
    pruefe(a2 != d and d not in a2 and a2 not in d,
           "Adresse und DID sind nicht gleich und nicht ineinander enthalten")



def pruefe_kreuzbezug(repo_wurzel: str) -> None:
    """Prueft, dass DID-Praefix und Seed-Konstruktor im Quelltext so existieren."""
    print("\n== Kreuzbezug zum Quelltext (E1.3/E1.4) ==")
    pfad = f"{repo_wurzel}/modules/atc-shivacore/service_space/src/did.rs"
    try:
        with open(pfad, encoding="utf-8") as f:
            quelle = f.read()
    except OSError as fehler:
        pruefe(False, f"did.rs lesbar ({fehler})")
        return
    treffer = [z for z in quelle.splitlines() if "ED25519_PREFIX" in z and "const" in z]
    if treffer:
        wert = treffer[0].split('"')[1] if '"' in treffer[0] else ""
        pruefe(wert == DID_PREFIX,
               f"did.rs:130 fuehrt ED25519_PREFIX = {wert!r}, Dokument nennt {DID_PREFIX!r}")
    else:
        pruefe(False, "did.rs fuehrt eine Konstante ED25519_PREFIX")
    pruefe("fn from_seed" in quelle,
           "did.rs bietet Ed25519Signer::from_seed(&[u8; 32])")
    pruefe("fn verifying_key" in quelle or "verifying_key" in quelle,
           "did.rs bildet den oeffentlichen Schluessel aus dem SigningKey")

def pruefe_dokument(repo_wurzel: str) -> None:
    """Prueft, dass das Dokument die Entscheidungen auch traegt."""
    print("\n== Dokument traegt die Entscheidungen ==")
    pfad = f"{repo_wurzel}/docs/identity/SPEC-RECOVERY-001.md"
    try:
        with open(pfad, encoding="utf-8") as f:
            text = f.read()
    except OSError as fehler:
        pruefe(False, f"Dokument lesbar ({fehler})")
        return
    for marke, was in [
        ("ADDRESS_ONLY", "E-6: ADDRESS_ONLY steht im Dokument"),
        ("NON_SIGNING", "E-6: NON_SIGNING steht im Dokument"),
        (LABEL_WALLET, "E1.3: HKDF-Label Wallet benannt (Suffix = key_version)"),
        (LABEL_IDENTITAET, "E1.3: HKDF-Label Identitaet benannt"),
        (LABEL_KNOTEN, "E1.3: HKDF-Label Knoten benannt"),
        ("PRK", "15.9: gemeinsamer HKDF-PRK benannt"),
        (SEED_WALLET, "15.9: Wallet-Seed eingefroren"),
        (ADRESSE_V2_15_9, "15.9: Adresse v2 eingefroren"),
        (ADRESSE_V1_15_9, "15.9: v1-Adresse als Negativanker eingefroren"),
        ("REC-T-61", "12: v1/v2-Negativtests in der Matrix"),
        (DID_PREFIX, "E1.4: DID-Praefix steht im Dokument"),
        (DOMAENE_ADRESSE_V2, "E1.4: Domaene der Adresse v2 benannt"),
    ]:
        pruefe(marke in text, was)



# --------------------------------------------------------------------------
# Eingefrorene Ableitung key_version = 2 (Abschnitt 15.9)
#
# Der Pruefer rechnet PBKDF2, HKDF und beide Adressformen nach. Die
# Ed25519-Kurvenpunkte kann er nicht rechnen (kein Krypto in der
# Standardbibliothek); sie stehen deshalb als feste Vektoren und werden ueber
# DID und Adresse geprueft, also ueber die Rechnung, die aus ihnen folgt.
# --------------------------------------------------------------------------

SATZ_15_9 = ("abandon " * 23 + "art").strip()
SEED_BIP39 = ("408b285c123836004f4b8842c89324c1f01382450c0d439af345ba7fc49acf70"
              "5489c6fc77dbd4e3dc1dd8cc6bc9f043db8ada1e243c4a0eafb290d399480840")
SEED_WALLET = "8779592f116a7e4a10780ec04e09fbc6fa3263b9d02cc2abf39fbca2c524b190"
SEED_IDENT = "76bb1b4ae2ff1bf0bb07814d389f06a3c1d33a980d27c07720bb987d6a876513"
SEED_KNOTEN = "7424cb067cc00dedef77720fcacc251adc60ec86a1225bc48beb66ad150c3898"
PUB_WALLET = "ce61a516ea04a58b3742ebebd81eeaa135901e7e6cfdd194406d3bb135f3b0a1"
ADRESSE_V2_15_9 = "ATC29CB48DFF96FEB380FD24F9A44EB42316"
ADRESSE_V1_15_9 = "ATCEA9DF048839BCA9C9C410E8C2ADEBDBA"
V1_HMAC = "2650593ba4d3e9576d7998e40c0ba712b61b85fc2f0cadd0d5b40aa9688f61c0"
V1_PUB = "453f844224e9b0c1cd4236d4dc00b831b9ea9a923fd9624ebf428a35704f16b9"
TC1_OKM = "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
TC3_OKM = "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"


def hkdf_extract(salt: bytes, ikm: bytes) -> bytes:
    return hmac.new(salt, ikm, hashlib.sha256).digest()


def hkdf_expand(prk: bytes, info: bytes, laenge: int) -> bytes:
    okm, t, i = b"", b"", 1
    while len(okm) < laenge:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest()
        okm += t
        i += 1
    return okm[:laenge]


def hkdf(salt: bytes, ikm: bytes, info: bytes, laenge: int) -> bytes:
    return hkdf_expand(hkdf_extract(salt, ikm), info, laenge)


def pruefe_ableitung_v2() -> None:
    print("\n== Eingefrorene Ableitung key_version = 2 (15.9) ==")

    # 1. HKDF gegen die Norm, damit die Handimplementierung belegt ist.
    pruefe(hkdf(bytes.fromhex("000102030405060708090a0b0c"), bytes.fromhex("0b" * 22),
                bytes.fromhex("f0f1f2f3f4f5f6f7f8f9"), 42).hex() == TC1_OKM,
           "HKDF RFC 5869 TC1 OKM stimmt")
    pruefe(hkdf(b"", bytes.fromhex("0b" * 22), b"", 42).hex() == TC3_OKM,
           "HKDF RFC 5869 TC3 OKM stimmt (leeres Salz, leeres info)")

    # 2. Wurzel und die drei Domaenen.
    seed = hashlib.pbkdf2_hmac("sha512", SATZ_15_9.encode(), b"mnemonic", 2048, 64)
    pruefe(seed.hex() == SEED_BIP39, "BIP39-Seed aus der Phrase stimmt")
    pruefe(len(SATZ_15_9.split()) == 24, "Testphrase hat 24 Woerter")

    salt = b"\x00" * 32
    for label, soll in ((LABEL_WALLET, SEED_WALLET),
                        (LABEL_IDENTITAET, SEED_IDENT),
                        (LABEL_KNOTEN, SEED_KNOTEN)):
        okm = hkdf(salt, seed, label.encode() + b"\x00", 32)
        pruefe(okm.hex() == soll, f"OKM {label} stimmt")
        ohne_null = hkdf(salt, seed, label.encode(), 32)
        pruefe(ohne_null != okm,
               f"{label}: info ohne Nullbyte ergibt einen anderen OKM")

    # 3. Gemeinsamer PRK, drei Ausgaben: Extract nur einmal.
    prk = hkdf_extract(salt, seed)
    for label, soll in ((LABEL_WALLET, SEED_WALLET),
                        (LABEL_IDENTITAET, SEED_IDENT),
                        (LABEL_KNOTEN, SEED_KNOTEN)):
        pruefe(hkdf_expand(prk, label.encode() + b"\x00", 32).hex() == soll,
               f"{label}: derselbe PRK, andere Expand-Domaene")
    pruefe(len({SEED_WALLET, SEED_IDENT, SEED_KNOTEN}) == 3,
           "drei verschiedene OKM, keine gemeinsame Ausgabe")

    # 4. Oeffentliche Kennungen aus den festen Vektoren.
    pub = bytes.fromhex(PUB_WALLET)
    pruefe(adresse_v2(pub) == ADRESSE_V2_15_9, "Adresse v2 aus dem Wallet-Pubkey stimmt")
    pruefe(did(pub) == DID_PREFIX + PUB_WALLET, "DID aus dem Wallet-Pubkey stimmt")
    pruefe(adresse_v1(bytes.fromhex(V1_HMAC)) == ADRESSE_V1_15_9,
           "v1-Adresse aus dem v1-HMAC stimmt")
    pruefe(len(ADRESSE_V1_15_9) == 35 and len(ADRESSE_V2_15_9) == 36,
           "35 gegen 36 Zeichen: die Fassungen sind beim Lesen unterscheidbar")

    # 5. Die vier Negativzusagen aus 15.9.
    pruefe(SEED_WALLET != V1_HMAC, "Wallet-Seed ist nicht der v1-HMAC")
    pruefe(PUB_WALLET != V1_PUB, "Wallet-Pubkey ist nicht der v1-Hash")
    pruefe(ADRESSE_V2_15_9 != ADRESSE_V1_15_9, "Adresse v2 ist nicht Adresse v1")
    pruefe(ADRESSE_V2_15_9[4:] != ADRESSE_V1_15_9[3:],
           "Adresskoerper v2 unterscheidet sich von v1 (kein Umrechnen)")

def main():
    print("== Tresor-Container GLBVAULT ==")
    summe = 0
    for name, offset, laenge in VAULT_FELDER:
        pruefe(offset == summe, f"{name}: Offset {offset} == {summe}")
        summe += laenge
    pruefe(summe == VAULT_HEADER_LEN,
           f"Summe der Felder = {summe} = VAULT_HEADER_LEN {VAULT_HEADER_LEN}")
    pruefe(VAULT_HEADER_LEN + VAULT_MAX_PLAINTEXT + VAULT_TAG_LEN
           == VAULT_MAX_BYTES,
           f"groesster Container = 80 + 1024 + 16 = {VAULT_MAX_BYTES} Byte")

    print("\n== Innere Nutzlast GLBS ==")
    summe = 0
    for name, offset, laenge in INNER_FELDER:
        pruefe(offset == summe, f"{name}: Offset {offset} == {summe}")
        summe += laenge
    pruefe(summe == INNER_FIXED_LEN,
           f"feste Felder = {summe} = INNER_FIXED_LEN {INNER_FIXED_LEN}")

    print("\n== Anmeldedatensatz ATID ==")
    summe = 0
    for name, offset, laenge in ATID_FELDER:
        pruefe(offset == summe, f"{name}: Offset {offset} == {summe}")
        summe += laenge
    pruefe(summe == ATID_LEN, f"Summe = {summe} = ATID_LEN {ATID_LEN}")

    print("\n== Roundtrip mit den Hoechstlaengen ==")
    inner_max = inner_pack(
        bytes(range(32)), 1, "n" * MAX_NETWORK_LEN,
        "u" * MAX_USER_ID_LEN, "A" * MAX_ADDRESS_LEN,
        "d" * MAX_DISPLAY_NAME_LEN)
    pruefe(len(inner_max) == 404,
           f"groesste Nutzlast der Fassung 1 = {len(inner_max)} Byte (erwartet 404)")
    geparst, fehler = inner_parse(inner_max)
    pruefe(fehler is None and geparst["user_id"] == "u" * MAX_USER_ID_LEN,
           "Hoechstlaengen lassen sich unveraendert zurueck lesen")

    print("\n== Beispiel: ein echter Datensatz ==")
    inner = inner_pack(bytes(range(32)), 1, "devnet", "michael",
                       "ATC" + "0" * 28 + "ABCD", "Michael")
    print(f"  Nutzlast: {len(inner)} Byte")
    salt, nonce = b"\x11" * 16, b"\x22" * 24
    kopf = vault_header(19456, 2, 1, salt, nonce, 1, 7, len(inner))
    container = kopf + b"\x00" * len(inner) + b"\x00" * VAULT_TAG_LEN
    print(f"  Container: {len(container)} Byte "
          f"({VAULT_HEADER_LEN} Kopf + {len(inner)} Geheimtext + {VAULT_TAG_LEN} Tag)")
    kopf_geparst, fehler = parse_vault_header(container[:VAULT_HEADER_LEN])
    pruefe(fehler is None, "Kopf des Beispiels wird gelesen")
    pruefe(kopf_geparst["generation"] == 7 and kopf_geparst["t_cost"] == 2,
           "Kopf traegt Generation und KDF-Parameter")
    pruefe(kopf_geparst["plaintext_len"] == len(inner),
           "Laengenfeld passt zur Nutzlast")

    print("\n== fail-closed: strukturell entscheidbare Faelle ==")
    faelle = [
        ("leerer Container", b"", "HEADER_LEN"),
        ("abgeschnittener Kopf", kopf[:70], "HEADER_LEN"),
        ("falsche Kennung", b"GLBVAULX" + kopf[8:], "MAGIC"),
        ("unbekannte Fassung", kopf[:8] + struct.pack("<H", 2) + kopf[10:], "VERSION"),
        ("unbekannte KDF", kopf[:10] + b"\x09" + kopf[11:], "KDF"),
        ("unbekannte AEAD", kopf[:11] + b"\x09" + kopf[12:], "AEAD"),
        ("reserviertes Feld gesetzt", kopf[:62] + b"\x01\x00" + kopf[64:], "RESERVED"),
        ("Nutzlastlaenge 0", kopf[:72] + struct.pack("<I", 0) + kopf[76:], "PLAINTEXT_LEN"),
        ("Nutzlastlaenge zu gross",
         kopf[:72] + struct.pack("<I", VAULT_MAX_PLAINTEXT + 1) + kopf[76:],
         "PLAINTEXT_LEN"),
        ("Generation 0", kopf[:64] + struct.pack("<Q", 0) + kopf[72:], "GENERATION"),
    ]
    for name, roh, erwartet in faelle:
        _, fehler = parse_vault_header(roh)
        pruefe(fehler == erwartet, f"{name} -> {erwartet} (war {fehler})")

    inner_faelle = [
        ("leere Nutzlast", b"", "MIN_LEN"),
        ("falsche Kennung", b"GLBX" + inner[4:], "MAGIC"),
        ("unbekannte Fassung", inner[:4] + struct.pack("<H", 2) + inner[6:], "VERSION"),
        ("gesetztes Flag", inner[:6] + b"\x01\x00" + inner[8:], "FLAGS"),
        ("unbekannte Sprache", inner[:42] + struct.pack("<H", 9) + inner[44:], "SPRACHE"),
        ("abgeschnittene Nutzlast", inner[:-3], "DISPLAY_NAME_TRUNCATED"),
        ("angehaengte Bytes", inner + b"\x00", "TRAILING_BYTES"),
        ("Netzwerklaenge ueber Grenze",
         inner[:44] + struct.pack("<H", MAX_NETWORK_LEN + 1) + inner[46:],
         "NETWORK_LEN"),
    ]
    for name, roh, erwartet in inner_faelle:
        _, fehler = inner_parse(roh)
        pruefe(fehler == erwartet, f"{name} -> {erwartet} (war {fehler})")

    pruefe_identifikatoren()
    pruefe_ableitung_v2()
    pruefe_kreuzbezug(WURZEL)
    pruefe_dokument(WURZEL)

    print()
    if FEHLER:
        print(f"{len(FEHLER)} Widersprueche — das Dokument und dieser Pruefer "
              f"stimmen nicht ueberein.")
        return 1
    print("Alle Zusagen halten. Layout, Grenzen und fail-closed-Faelle "
          "stimmen mit dem Dokument ueberein.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
