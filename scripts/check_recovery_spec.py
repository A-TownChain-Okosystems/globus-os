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

import struct
import sys

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
