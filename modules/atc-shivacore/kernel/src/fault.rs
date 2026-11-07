// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// FAULT-001 — Was bei einem Prozessorfehler passiert
//
// Bis hierher galt fuer jede Ausnahme dasselbe: loggen und weiter. Bei einem
// Seitenfehler aus Ring 3 heisst "weiter" aber, an dieselbe Anweisung
// zurueckzukehren, die eben gefehlt hat — die Ausnahme loest sofort wieder aus
// und die Maschine haengt. Ein abstuerzendes Programm nahm damit das ganze
// System mit, und genau das darf nach der Anforderung "ein Programmabsturz im
// User Space darf niemals den Kernel reissen" nicht passieren.
//
// Diese Datei entscheidet nur, WER den Fehler bezahlt: der Prozess oder der
// Kernel. Sie fasst keinen Prozessorzustand an, damit die Entscheidung ohne
// QEMU pruefbar bleibt. Die Ausfuehrung steht in `interrupts.rs`.
// ─────────────────────────────────────────────────────────────────────────

/// Aus welchem Ring der Fehler kam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Ring 3: ein Programm. Sein Fehler, seine Rechnung.
    User,
    /// Ring 0: der Kernel selbst. Hier ist nichts mehr zu retten.
    Kernel,
}

impl Origin {
    /// Liest den Ring aus dem Code-Segment des Ausnahme-Rahmens.
    ///
    /// Die unteren zwei Bits von CS sind die Requested Privilege Level. Ein
    /// Fehler "aus Ring 3" ist damit eine Eigenschaft des Rahmens, nicht eine
    /// Annahme ueber den Aufrufer.
    pub fn from_cs(cs: u64) -> Self {
        if cs & 0b11 == 0 {
            Origin::Kernel
        } else {
            Origin::User
        }
    }
}

/// Welche Ausnahme den Weg hierher gefunden hat.
///
/// Die drei unterscheiden sich nicht in der Behandlung, aber im Bericht: "der
/// Prozess hat auf eine fehlende Seite gegriffen" ist eine andere Aussage als
/// "der Prozess hat eine privilegierte Anweisung versucht". Ohne diese
/// Unterscheidung steht im Log nur, dass etwas passiert ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionKind {
    /// Seitenfehler (Vektor 14), die CPU legt einen Fehlercode dazu.
    PageFault,
    /// Schutzverletzung (Vektor 13), ebenfalls mit Fehlercode.
    GeneralProtection,
    /// Ungueltiger Opcode (Vektor 6), ohne Fehlercode.
    InvalidOpcode,
}

impl ExceptionKind {
    /// Name fuer das Log.
    pub fn name(&self) -> &'static str {
        match self {
            ExceptionKind::PageFault => "Seitenfehler",
            ExceptionKind::GeneralProtection => "Schutzverletzung",
            ExceptionKind::InvalidOpcode => "Ungueltiger Opcode",
        }
    }

    /// Liest die Art aus der Zahl, die das Assembly uebergibt.
    ///
    /// Unbekannte Zahlen werden als Seitenfehler behandelt: eine falsche
    /// Beschriftung im Log ist harmlos, ein falscher Rahmen nicht — und der
    /// Rahmen haengt bei allen drei gleich.
    pub fn from_id(id: u32) -> Self {
        match id {
            1 => ExceptionKind::GeneralProtection,
            2 => ExceptionKind::InvalidOpcode,
            _ => ExceptionKind::PageFault,
        }
    }

    /// Legt die CPU zu dieser Ausnahme einen Fehlercode auf den Stapel?
    ///
    /// Das entscheidet, welcher Trampolin zustaendig ist: wer hier falsch
    /// liegt, verschiebt den ganzen Rahmen und liest Unsinn als RIP.
    pub fn has_error_code(&self) -> bool {
        !matches!(self, ExceptionKind::InvalidOpcode)
    }
}

/// Was mit dem Fehler geschieht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultAction {
    /// Den fehlerhaften Prozess beenden und weiterlaufen.
    TerminateProcess,
    /// Den Kernel anhalten. Ein Fehler im Ring 0 ist ein Fehler im System.
    PanicKernel,
}

/// Die Entscheidung. Bewusst eine reine Funktion ohne Seiteneffekt.
pub fn decide(_kind: ExceptionKind, origin: Origin) -> FaultAction {
    match origin {
        Origin::User => FaultAction::TerminateProcess,
        Origin::Kernel => FaultAction::PanicKernel,
    }
}

/// Die Bits des Fehlercodes, den die CPU beim Seitenfehler mitliefert.
///
/// Gelesen wird nur, was fuer die Diagnose zaehlt. Wer den Code als Zahl loggt,
/// muss beim naechsten Mal wieder nachschlagen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FaultReport {
    /// Die Seite war nicht vorhanden (sonst: Schutzverletzung).
    pub not_present: bool,
    /// Es wurde geschrieben (sonst: gelesen).
    pub write: bool,
    /// Der Zugriff kam aus Ring 3.
    pub from_user: bool,
    /// Beim Holen einer Anweisung (sonst: Datenzugriff).
    pub instruction_fetch: bool,
}

impl FaultReport {
    /// Zerlegt den Fehlercode.
    pub fn from_error_code(code: u64) -> Self {
        Self {
            not_present: code & 0b0001 == 0,
            write: code & 0b0010 != 0,
            from_user: code & 0b0100 != 0,
            instruction_fetch: code & 0b1000_0000 != 0,
        }
    }

    /// Kurzform fuer das Log.
    pub fn describe(&self) -> &'static str {
        match (self.not_present, self.write, self.instruction_fetch) {
            (true, false, false) => "Seite nicht vorhanden, Lesen",
            (true, true, false) => "Seite nicht vorhanden, Schreiben",
            (true, false, true) => "Seite nicht vorhanden, Befehlsabruf",
            (false, false, false) => "Schutzverletzung, Lesen",
            (false, true, false) => "Schutzverletzung, Schreiben",
            (false, false, true) => "Schutzverletzung, Befehlsabruf",
            // Schreiben beim Befehlsabruf gibt es nicht; die CPU liefert das
            // Bit trotzdem mit. Nicht als unmoeglich behandeln, sondern sagen.
            _ => "untypische Kombination",
        }
    }
}

/// Zaehler ueber alle Fehler seit dem Start.
///
/// Getrennt nach Herkunft, weil die Zahlen verschiedene Fragen beantworten:
/// "wie oft hat ein Programm danebengegriffen" und "wie nah war der Kernel am
/// Absturz".
#[derive(Debug, Default)]
pub struct FaultCounters {
    user_faults: u64,
    kernel_faults: u64,
    terminated: u64,
}

/// Was gezaehlt wurde, nachdem ein Fehler verbucht ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    pub user_faults: u64,
    pub kernel_faults: u64,
    pub terminated: u64,
}

impl FaultCounters {
    /// Neuer, leerer Zaehler.
    pub const fn new() -> Self {
        Self {
            user_faults: 0,
            kernel_faults: 0,
            terminated: 0,
        }
    }

    /// Verbucht einen Fehler samt Entscheidung.
    pub fn record(&mut self, origin: Origin, action: FaultAction) -> Recorded {
        match origin {
            Origin::User => self.user_faults += 1,
            Origin::Kernel => self.kernel_faults += 1,
        }
        if action == FaultAction::TerminateProcess {
            self.terminated += 1;
        }
        Recorded {
            user_faults: self.user_faults,
            kernel_faults: self.kernel_faults,
            terminated: self.terminated,
        }
    }

    /// Stand abfragen, ohne zu zaehlen.
    pub fn snapshot(&self) -> Recorded {
        Recorded {
            user_faults: self.user_faults,
            kernel_faults: self.kernel_faults,
            terminated: self.terminated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_three_segment_means_user() {
        // 0x1B = Ring 3, typisches Userspace-CS auf x86-64.
        assert_eq!(Origin::from_cs(0x1B), Origin::User);
        assert_eq!(Origin::from_cs(0x23), Origin::User);
    }

    #[test]
    fn ring_zero_segment_means_kernel() {
        // 0x08 = Kernel-CS.
        assert_eq!(Origin::from_cs(0x08), Origin::Kernel);
        assert_eq!(Origin::from_cs(0x10), Origin::Kernel);
    }

    #[test]
    fn a_user_fault_costs_the_process_not_the_system() {
        assert_eq!(
            decide(ExceptionKind::PageFault, Origin::User),
            FaultAction::TerminateProcess
        );
    }

    #[test]
    fn a_kernel_fault_still_stops_the_kernel() {
        assert_eq!(
            decide(ExceptionKind::InvalidOpcode, Origin::Kernel),
            FaultAction::PanicKernel
        );
    }

    #[test]
    fn the_ring_decides_for_every_kind_the_same_way() {
        // Die Art der Ausnahme aendert die Behandlung nicht: Ring 3 zahlt der
        // Prozess, Ring 0 der Kernel. Sonst entstuende ein Schlupfloch, das
        // nur davon abhaengt, welche Anweisung ein Programm benutzt.
        for kind in [
            ExceptionKind::PageFault,
            ExceptionKind::GeneralProtection,
            ExceptionKind::InvalidOpcode,
        ] {
            assert_eq!(decide(kind, Origin::User), FaultAction::TerminateProcess);
            assert_eq!(decide(kind, Origin::Kernel), FaultAction::PanicKernel);
        }
    }

    #[test]
    fn only_the_invalid_opcode_lacks_an_error_code() {
        // Bestimmt, welcher Trampolin zustaendig ist. Wer hier irrt, verschiebt
        // den Rahmen und liest Muell als RIP.
        assert!(ExceptionKind::PageFault.has_error_code());
        assert!(ExceptionKind::GeneralProtection.has_error_code());
        assert!(!ExceptionKind::InvalidOpcode.has_error_code());
    }

    #[test]
    fn the_kind_survives_the_trip_through_assembly() {
        assert_eq!(ExceptionKind::from_id(0), ExceptionKind::PageFault);
        assert_eq!(ExceptionKind::from_id(1), ExceptionKind::GeneralProtection);
        assert_eq!(ExceptionKind::from_id(2), ExceptionKind::InvalidOpcode);
        // Unbekannt faellt auf den Rahmen zu, der keinen Fehlercode verliert.
        assert_eq!(ExceptionKind::from_id(99), ExceptionKind::PageFault);
    }

    #[test]
    fn each_kind_has_its_own_name() {
        assert_eq!(ExceptionKind::PageFault.name(), "Seitenfehler");
        assert_eq!(ExceptionKind::GeneralProtection.name(), "Schutzverletzung");
        assert_eq!(ExceptionKind::InvalidOpcode.name(), "Ungueltiger Opcode");
    }

    #[test]
    fn the_error_code_is_read_the_right_way_round() {
        // Bit 0 heisst "vorhanden", nicht "nicht vorhanden" — genau hier
        // verwechselt man sich beim Lesen der Intel-Doku.
        let absent = FaultReport::from_error_code(0b0000);
        assert!(absent.not_present);
        let protection = FaultReport::from_error_code(0b0001);
        assert!(!protection.not_present);
    }

    #[test]
    fn write_and_user_bits_are_read() {
        let report = FaultReport::from_error_code(0b0000_0110);
        assert!(report.write);
        assert!(report.from_user);
        assert!(report.not_present);
        assert!(!report.instruction_fetch);
    }

    #[test]
    fn the_description_says_what_happened() {
        assert_eq!(
            FaultReport::from_error_code(0b0000).describe(),
            "Seite nicht vorhanden, Lesen"
        );
        assert_eq!(
            FaultReport::from_error_code(0b0010).describe(),
            "Seite nicht vorhanden, Schreiben"
        );
        assert_eq!(
            FaultReport::from_error_code(0b0000_0001).describe(),
            "Schutzverletzung, Lesen"
        );
    }

    #[test]
    fn counters_separate_the_two_questions() {
        let mut counters = FaultCounters::new();
        counters.record(Origin::User, FaultAction::TerminateProcess);
        counters.record(Origin::User, FaultAction::TerminateProcess);
        let recorded = counters.record(Origin::Kernel, FaultAction::PanicKernel);
        assert_eq!(
            recorded,
            Recorded {
                user_faults: 2,
                kernel_faults: 1,
                terminated: 2
            }
        );
    }

    #[test]
    fn asking_twice_does_not_count_twice() {
        let mut counters = FaultCounters::new();
        counters.record(Origin::User, FaultAction::TerminateProcess);
        let first = counters.snapshot();
        let second = counters.snapshot();
        assert_eq!(first, second);
        assert_eq!(second.user_faults, 1);
    }
}
