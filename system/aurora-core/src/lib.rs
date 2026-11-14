#![doc = "Aurora AI control-plane contracts for GlobusOS."]

/// Capability-, Policy-, Approval-, Agent-, Tool-, Model- und Runtime-Traits.
pub mod contracts;
/// Fehlertyp der gesamten Control Plane.
pub mod errors;
/// Zustandsmaschine einer Anfrage.
pub mod state;
/// Typisierte Kennungen und das Request-/Response-Paar.
pub mod types;

pub use contracts::*;
pub use errors::AuroraError;
pub use state::StateMachine;
pub use types::{
    AgentId, ApprovalId, AuroraRequest, AuroraResponse, CapabilityId, EventId, ModelId, PolicyId,
    RequestId, SessionId, SkillId, ToolId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ergebnis einer Capability-Pruefung.
pub enum Decision {
    /// Erlaubt, die Ausfuehrung darf weiterlaufen.
    Allow,
    /// Abgelehnt und endgueltig.
    Deny,
    /// Braucht eine menschliche Freigabe.
    ApprovalRequired,
}

impl Decision {
    /// Nur `Deny` ist endgueltig; keine spaetere Entscheidung hebt sie auf.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Deny)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand einer Freigabeanfrage.
pub enum ApprovalState {
    /// Keine Freigabe noetig.
    NotRequired,
    /// Angefragt, noch nicht entschieden.
    Pending,
    /// Freigegeben.
    Granted,
    /// Abgelehnt.
    /// Abgelehnt.
    Denied,
    /// Frist abgelaufen, ohne Entscheidung.
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Lebenszyklus einer Anfrage.
pub enum RequestStatus {
    /// Angelegt, noch nicht eingereiht.
    Created,
    /// Eingereiht, wartet auf Ausfuehrung.
    Queued,
    /// In Ausfuehrung.
    Running,
    /// Wartet auf eine menschliche Freigabe.
    WaitingApproval,
    /// Wartet auf eine freie Ressource.
    WaitingResource,
    /// Angehalten, Zustand bleibt erhalten.
    Paused,
    /// Ergebnis wird geprueft.
    Verifying,
    /// Erfolgreich abgeschlossen.
    Completed,
    /// Fehlgeschlagen.
    Failed,
    /// Vom Aufrufer abgebrochen.
    Cancelled,
    Denied,
    /// Zeitlimit ueberschritten.
    Timeout,
    /// Ressourcen erschoepft.
    ResourceExhausted,
    /// Sicherheitsregel verletzt.
    SecurityViolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Risikoklasse einer Capability, L0 harmlos bis L6 am hoechsten.
pub enum RiskLevel {
    /// Kein Risiko.
    L0,
    /// Sehr geringes Risiko.
    L1,
    /// Geringes Risiko.
    L2,
    /// Mittleres Risiko.
    L3,
    /// Erhoehtes Risiko.
    L4,
    /// Hohes Risiko.
    L5,
    /// Hoechstes Risiko, nur mit Freigabe.
    L6,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_is_terminal() {
        assert!(Decision::Deny.is_terminal());
        assert!(!Decision::ApprovalRequired.is_terminal());
    }
}
