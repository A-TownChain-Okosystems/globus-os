//! SC-006: Zuordnung von Geraete-IRQs zu Vektoren.
//!
//! Vorher stand der Vektor des Timers als Zahl im Bootpfad. Fuer eine zweite
//! Line haette es keine Zuordnung gegeben: niemand haette gewusst, welcher
//! Vektor noch frei ist, und ein zweiter Eintrag haette den ersten ueberschrieben.
//!
//! Diese Tabelle haelt fest, welche Line welchen Vektor belegt. Der Vektor 0..32
//! ist Prozessor-Exceptions vorbehalten und wird nie vergeben; bereits belegte
//! Vektoren (etwa der des Timers) werden uebersprungen, statt sie zu teilen.

/// Erster Vektor, den die automatische Vergabe anfasst. Darunter liegen die
/// Prozessor-Exceptions (0..32) und ab 0x20 der Timer, dessen Vektor aus dem
/// IDT-Index stammt.
pub const FIRST_DEVICE_VECTOR: u8 = 0x30;
/// Ab hier ist ein Vektor ueberhaupt routbar; darunter liegen ausschliesslich
/// Prozessor-Exceptions.
pub const FIRST_ROUTABLE_VECTOR: u8 = 0x20;
/// Letzter nutzbarer Vektor; 0xFF ist der Spurious-Vektor.
pub const LAST_DEVICE_VECTOR: u8 = 0xEF;
/// So viele Eintraege hat die Redirection Table eines IOAPIC hoechstens.
pub const MAX_ROUTES: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    /// Vektor liegt im Bereich der Prozessor-Exceptions oder hinter dem Ende.
    VectorNotUsable,
    /// Fuer diese Line steht schon ein Eintrag.
    IrqAlreadyRouted,
    /// Der Vektor ist bereits einer anderen Line zugeordnet.
    VectorTaken,
    /// Der Vektorbereich ist ausgeschoepft.
    NoVectorLeft,
    /// Kein Platz mehr in der Tabelle.
    TableFull,
}

/// Eine Line mit ihrem Vektor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqRoute {
    pub irq: u8,
    pub gsi: u32,
    pub vector: u8,
}

/// Die belegten Zuordnungen.
pub struct IrqRouting {
    routes: [Option<IrqRoute>; MAX_ROUTES],
}

impl Default for IrqRouting {
    fn default() -> Self {
        Self::new()
    }
}

impl IrqRouting {
    pub fn new() -> Self {
        Self {
            routes: [None; MAX_ROUTES],
        }
    }

    /// Traegt eine Line mit fest vorgegebenem Vektor ein, etwa den Timer, dessen
    /// Vektor aus dem IDT-Index abgeleitet ist. Der Vektor gilt danach als belegt.
    pub fn assign_fixed(&mut self, irq: u8, gsi: u32, vector: u8) -> Result<IrqRoute, RouteError> {
        if !is_routable(vector) {
            return Err(RouteError::VectorNotUsable);
        }
        if self.route_for_irq(irq).is_some() {
            return Err(RouteError::IrqAlreadyRouted);
        }
        if self.route_for_vector(vector).is_some() {
            return Err(RouteError::VectorTaken);
        }
        let route = IrqRoute { irq, gsi, vector };
        self.push(route)?;
        Ok(route)
    }

    /// Vergibt den naechsten freien Vektor an eine Line.
    pub fn assign_next_free(&mut self, irq: u8, gsi: u32) -> Result<IrqRoute, RouteError> {
        if self.route_for_irq(irq).is_some() {
            return Err(RouteError::IrqAlreadyRouted);
        }
        let vector = self.next_free_vector().ok_or(RouteError::NoVectorLeft)?;
        let route = IrqRoute { irq, gsi, vector };
        self.push(route)?;
        Ok(route)
    }

    /// Naechster freier Geraetevektor, ohne einen zu belegen.
    pub fn next_free_vector(&self) -> Option<u8> {
        (FIRST_DEVICE_VECTOR..=LAST_DEVICE_VECTOR)
            .find(|vector| self.route_for_vector(*vector).is_none())
    }

    pub fn route_for_irq(&self, irq: u8) -> Option<IrqRoute> {
        self.routes
            .iter()
            .flatten()
            .copied()
            .find(|route| route.irq == irq)
    }

    /// Der Interrupt-Pfad kennt nur den Vektor, deshalb dieser Weg zurueck.
    pub fn route_for_vector(&self, vector: u8) -> Option<IrqRoute> {
        self.routes
            .iter()
            .flatten()
            .copied()
            .find(|route| route.vector == vector)
    }

    pub fn route_for_gsi(&self, gsi: u32) -> Option<IrqRoute> {
        self.routes
            .iter()
            .flatten()
            .copied()
            .find(|route| route.gsi == gsi)
    }

    pub fn len(&self) -> usize {
        self.routes.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn push(&mut self, route: IrqRoute) -> Result<(), RouteError> {
        let slot = self
            .routes
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or(RouteError::TableFull)?;
        *slot = Some(route);
        Ok(())
    }
}

/// Routbar ist alles ab dem ersten Nicht-Exception-Vektor. Der Timer liegt mit
/// 0x20 darin, wird aber von der automatischen Vergabe nicht angefasst, weil
/// die erst bei 0x30 beginnt.
pub fn is_routable(vector: u8) -> bool {
    (FIRST_ROUTABLE_VECTOR..=LAST_DEVICE_VECTOR).contains(&vector)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_device_line_gets_the_first_device_vector() {
        let mut routing = IrqRouting::new();
        let route = routing.assign_next_free(1, 1).expect("Vektor");
        assert_eq!(route.vector, FIRST_DEVICE_VECTOR);
        assert_eq!(routing.len(), 1);
    }

    #[test]
    fn second_line_gets_the_next_vector_and_not_the_first() {
        let mut routing = IrqRouting::new();
        routing.assign_next_free(1, 1).unwrap();
        let route = routing.assign_next_free(4, 4).unwrap();
        assert_eq!(route.vector, FIRST_DEVICE_VECTOR + 1);
    }

    #[test]
    fn the_timer_keeps_its_vector_and_devices_start_above_it() {
        // Der Timer haengt an 0x20 (aus dem IDT-Index abgeleitet). Der Eintrag
        // ist zulaessig, und die automatische Vergabe beginnt darueber bei 0x30.
        let mut routing = IrqRouting::new();
        let timer = routing.assign_fixed(0, 2, 0x20).expect("Timer-Route");
        assert_eq!(timer.vector, 0x20);
        let device = routing.assign_next_free(1, 1).unwrap();
        assert_eq!(device.vector, FIRST_DEVICE_VECTOR);
        assert_eq!(
            routing.assign_fixed(2, 2, 0x20),
            Err(RouteError::VectorTaken),
            "der Timer-Vektor darf nicht geteilt werden"
        );
    }

    #[test]
    fn exception_vectors_are_never_handed_out() {
        let mut routing = IrqRouting::new();
        assert_eq!(
            routing.assign_fixed(9, 9, 0x0D),
            Err(RouteError::VectorNotUsable)
        );
        assert!(is_routable(0x30));
        assert!(is_routable(0x20), "erster Vektor nach den Exceptions");
        assert!(!is_routable(0x1F), "letzter Exception-Vektor");
        assert!(!is_routable(0xF0));
    }

    #[test]
    fn a_line_is_routed_only_once() {
        let mut routing = IrqRouting::new();
        routing.assign_next_free(4, 4).unwrap();
        assert_eq!(
            routing.assign_next_free(4, 4),
            Err(RouteError::IrqAlreadyRouted)
        );
    }

    #[test]
    fn a_vector_is_never_shared_between_lines() {
        let mut routing = IrqRouting::new();
        routing.assign_fixed(4, 4, 0x30).unwrap();
        assert_eq!(
            routing.assign_fixed(5, 5, 0x30),
            Err(RouteError::VectorTaken)
        );
    }

    #[test]
    fn the_path_back_finds_the_line_by_vector_and_by_gsi() {
        // Der Interrupt-Pfad hat nur den Vektor, der IOAPIC nur die GSI.
        let mut routing = IrqRouting::new();
        let route = routing.assign_next_free(4, 4).unwrap();
        assert_eq!(routing.route_for_vector(route.vector), Some(route));
        assert_eq!(routing.route_for_gsi(4), Some(route));
        assert_eq!(routing.route_for_irq(4), Some(route));
        assert_eq!(routing.route_for_vector(0x99), None);
    }

    #[test]
    fn the_table_fills_up_and_says_so() {
        // Das Vektorfenster umfasst 193 Vektoren, die Tabelle nur MAX_ROUTES
        // Eintraege (so viele Zeilen hat die Redirection Table eines IOAPIC).
        // Die bindende Grenze ist deshalb die Tabelle, nicht der Vektorraum:
        // NoVectorLeft ist mit dieser Groesse nicht erreichbar, und der Test
        // behauptet es auch nicht.
        let window = usize::from(LAST_DEVICE_VECTOR - FIRST_DEVICE_VECTOR) + 1;
        assert!(window > MAX_ROUTES);
        let mut routing = IrqRouting::new();
        for irq in 0..MAX_ROUTES {
            routing.assign_next_free(irq as u8, irq as u32).unwrap();
        }
        assert_eq!(routing.len(), MAX_ROUTES);
        assert_eq!(
            routing.assign_next_free(200, 200),
            Err(RouteError::TableFull)
        );
        assert!(
            routing.next_free_vector().is_some(),
            "Vektoren waeren noch frei, nur keine Zeile mehr"
        );
    }
}
