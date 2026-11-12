// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Keccak-256 (Original-Keccak, Padding-Byte `0x01`) in reinem Rust.
//!
//! Warum das im Kernel steht: das Audit-Log braucht einen Hash, dessen
//! Kollisionsresistenz nicht davon abhaengt, dass niemand nachrechnet. Vorher
//! lief dort FNV-1a ueber 32 Bit, aufgefuellt auf 32 Bytes — eine Pruefsumme,
//! kein Sicherheitsmerkmal: 2^32 Operationen reichen fuer eine Kollision, und
//! damit ist die Hash-Kette des Audit-Logs faelschbar.
//!
//! Keccak-256 ist hier bewusst selbst implementiert statt als Abhaengigkeit
//! eingebunden: der Kernel soll keine Krypto-Crate fuer 32 Byte Ausgabe ziehen,
//! und die Implementierung ist gegen bekannte Testvektoren geprueft.

const ROUNDS: usize = 24;
const RATE: usize = 136; // 1088 Bit Rate bei 512 Bit Capacity -> 256 Bit Ausgabe

const RC: [u64; ROUNDS] = [
    0x0000_0000_0000_0001,
    0x0000_0000_0000_8082,
    0x8000_0000_0000_808a,
    0x8000_0000_8000_8000,
    0x0000_0000_0000_808b,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8009,
    0x0000_0000_0000_008a,
    0x0000_0000_0000_0088,
    0x0000_0000_8000_8009,
    0x0000_0000_8000_000a,
    0x0000_0000_8000_808b,
    0x8000_0000_0000_008b,
    0x8000_0000_0000_8089,
    0x8000_0000_0000_8003,
    0x8000_0000_0000_8002,
    0x8000_0000_0000_0080,
    0x0000_0000_0000_800a,
    0x8000_0000_8000_000a,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8080,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8008,
];

const ROTC: [u32; 24] = [
    1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 2, 14, 27, 41, 56, 8, 25, 43, 62, 18, 39, 61, 20, 44,
];

const PILN: [usize; 24] = [
    10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4, 15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
];

fn keccak_f1600(a: &mut [u64; 25]) {
    for round in 0..ROUNDS {
        // theta
        let mut c = [0u64; 5];
        for (x, slot) in c.iter_mut().enumerate() {
            *slot = a[x] ^ a[x + 5] ^ a[x + 10] ^ a[x + 15] ^ a[x + 20];
        }
        for x in 0..5 {
            let d = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
            for y in 0..5 {
                a[x + 5 * y] ^= d;
            }
        }
        // rho + pi
        let mut last = a[1];
        for i in 0..ROUNDS {
            let j = PILN[i];
            let tmp = a[j];
            a[j] = last.rotate_left(ROTC[i]);
            last = tmp;
        }
        // chi
        for y in 0..5 {
            let mut row = [0u64; 5];
            row.copy_from_slice(&a[5 * y..5 * y + 5]);
            for x in 0..5 {
                a[x + 5 * y] = row[x] ^ ((!row[(x + 1) % 5]) & row[(x + 2) % 5]);
            }
        }
        // iota
        a[0] ^= RC[round];
    }
}

fn absorb(state: &mut [u64; 25], block: &[u8; RATE]) {
    for (i, byte) in block.iter().enumerate() {
        state[i / 8] ^= (*byte as u64) << (8 * (i % 8));
    }
    keccak_f1600(state);
}

/// Keccak-256 ueber beliebig viele Bytes.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut state = [0u64; 25];

    let mut chunks = data.chunks_exact(RATE);
    for chunk in &mut chunks {
        let mut block = [0u8; RATE];
        block.copy_from_slice(chunk);
        absorb(&mut state, &block);
    }

    // Padding nach Original-Keccak: 0x01 am Ende der Nachricht, 0x80 als letztes
    // Byte des Blocks. (SHA-3 nutzt 0x06 — bewusst nicht uebernommen, weil die
    // Audit-Kette dann nicht gegen die verbreiteten Keccak-256-Vektoren pruefbar
    // waere.)
    let rest = chunks.remainder();
    let mut block = [0u8; RATE];
    block[..rest.len()].copy_from_slice(rest);
    block[rest.len()] ^= 0x01;
    block[RATE - 1] ^= 0x80;
    absorb(&mut state, &block);

    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = ((state[i / 8] >> (8 * (i % 8))) & 0xFF) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::keccak256;
    use alloc::string::String;

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::new();
        for b in bytes {
            s.push_str(&alloc::format!("{:02x}", b));
        }
        s
    }

    #[test]
    fn empty_input_matches_known_vector() {
        assert_eq!(
            hex(&keccak256(b"")),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn abc_matches_known_vector() {
        assert_eq!(
            hex(&keccak256(b"abc")),
            "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }

    #[test]
    fn multi_block_input_is_not_truncated() {
        // 200 Bytes: ein voller 136-Byte-Block plus Rest. Wuerde der Rest beim
        // Absorbieren ignoriert, haetten beide Nachrichten denselben Hash.
        let mut a = alloc::vec![b'x'; 200];
        let mut b = a.clone();
        b[150] = b'y';
        assert_ne!(keccak256(&a), keccak256(&b));
        a[199] = b'z';
        assert_ne!(keccak256(&a), keccak256(&b));
    }

    #[test]
    fn rate_boundary_is_padded() {
        // Genau ein Block mit Nullbytes darf nicht wie die leere Nachricht
        // hashen: das waere ein Padding-Fehler.
        let zeros = [0u8; 136];
        assert_ne!(keccak256(&zeros), keccak256(&[]));
    }

    #[test]
    fn different_lengths_produce_different_hashes() {
        assert_ne!(keccak256(b"a"), keccak256(b"aa"));
        assert_ne!(keccak256(b"shivacore"), keccak256(b"ShivaCore"));
    }
}
