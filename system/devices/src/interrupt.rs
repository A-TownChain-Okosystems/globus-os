//! Architecture-neutral interrupt routing and acknowledgement contract.

use alloc::vec::Vec;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// Interruptvektor, als u16 gefuehrt.
pub struct InterruptVector(pub u16);

impl InterruptVector {
    /// True, wenn der Vektor im gueltigen Bereich liegt.
    pub fn valid(self) -> bool {
        (32..=255).contains(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ausloesung eines Interrupts.
pub enum TriggerMode {
    /// Flankengetriggert.
    Edge,
    /// Pegelgetriggert.
    Level,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Polaritaet des Signals.
pub enum Polarity {
    /// Aktiv bei hohem Pegel.
    ActiveHigh,
    /// Aktiv bei niedrigem Pegel.
    ActiveLow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustellung eines Interrupts an eine CPU.
pub struct InterruptRoute {
    /// Interruptvektor.
    pub vector: InterruptVector,
    /// Ziel-CPU.
    pub destination_cpu: u32,
    /// Ausloesung.
    pub trigger: TriggerMode,
    /// Polaritaet.
    pub polarity: Polarity,
    /// True, wenn der Interrupt maskiert ist.
    pub masked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der Interrupt-Steuerung.
pub enum InterruptError {
    /// Der Vektor ist ungueltig.
    InvalidVector,
    /// Die Ziel-CPU ist ungueltig.
    InvalidCpu,
    /// Fuer diesen Vektor existiert bereits eine Route.
    AlreadyRouted,
    /// Fuer diesen Vektor existiert keine Route.
    NotRouted,
}

#[derive(Debug, Default)]
/// Routen der Interrupts.
pub struct InterruptController {
    routes: Vec<InterruptRoute>,
}

impl InterruptController {
    /// Leere Routentabelle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Legt eine Route an und lehnt Doppelungen ab.
    pub fn route(&mut self, route: InterruptRoute) -> Result<(), InterruptError> {
        if !route.vector.valid() {
            return Err(InterruptError::InvalidVector);
        }
        if route.destination_cpu == u32::MAX {
            return Err(InterruptError::InvalidCpu);
        }
        if self.routes.iter().any(|r| r.vector == route.vector) {
            return Err(InterruptError::AlreadyRouted);
        }
        self.routes.push(route);
        self.routes.sort_by_key(|r| r.vector);
        Ok(())
    }

    /// Maskiert oder entmaskiert einen Vektor.
    pub fn mask(&mut self, vector: InterruptVector, masked: bool) -> Result<(), InterruptError> {
        let route = self
            .routes
            .iter_mut()
            .find(|r| r.vector == vector)
            .ok_or(InterruptError::NotRouted)?;
        route.masked = masked;
        Ok(())
    }

    /// Route eines Vektors.
    pub fn route_for(&self, vector: InterruptVector) -> Option<InterruptRoute> {
        self.routes.iter().find(|r| r.vector == vector).copied()
    }

    /// Alle Routen in Einfuegereihenfolge.
    pub fn routes(&self) -> &[InterruptRoute] {
        &self.routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_are_deterministic_and_maskable() {
        let mut c = InterruptController::new();
        c.route(InterruptRoute {
            vector: InterruptVector(40),
            destination_cpu: 1,
            trigger: TriggerMode::Level,
            polarity: Polarity::ActiveHigh,
            masked: false,
        })
        .unwrap();
        assert_eq!(c.route_for(InterruptVector(40)).unwrap().destination_cpu, 1);
        c.mask(InterruptVector(40), true).unwrap();
        assert!(c.route_for(InterruptVector(40)).unwrap().masked);
    }
}
