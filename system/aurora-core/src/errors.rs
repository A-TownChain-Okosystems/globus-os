#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler, die die Aurora-Runtime meldet.
pub enum AuroraError {
    /// Die Anfrage ist leer oder unbrauchbar.
    InvalidRequest,
    /// Der Aufrufer ist nicht authentifiziert.
    Unauthorized,
    /// Die Capability deckt die Aktion nicht.
    CapabilityDenied,
    /// Eine Policy verbietet die Aktion.
    PolicyDenied,
    /// Es fehlt eine menschliche Freigabe.
    ApprovalRequired,
    /// Die Ausfuehrung ist fehlgeschlagen.
    ExecutionFailed,
    /// Das Ergebnis hat die Pruefung nicht bestanden.
    VerificationFailed,
    /// Das Zeitlimit wurde ueberschritten.
    Timeout,
    /// Eine Ressource ist erschoepft.
    ResourceExhausted,
    /// Eine Sicherheitsregel wurde verletzt.
    SecurityViolation,
    /// Dieser Zustandswechsel ist nicht erlaubt.
    InvalidStateTransition,
    /// Interner Fehler ohne genauere Einordnung.
    Internal,
}

impl core::fmt::Display for AuroraError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidRequest => "invalid request",
            Self::Unauthorized => "unauthorized",
            Self::CapabilityDenied => "capability denied",
            Self::PolicyDenied => "policy denied",
            Self::ApprovalRequired => "approval required",
            Self::ExecutionFailed => "execution failed",
            Self::VerificationFailed => "verification failed",
            Self::Timeout => "timeout",
            Self::ResourceExhausted => "resource exhausted",
            Self::SecurityViolation => "security violation",
            Self::InvalidStateTransition => "invalid state transition",
            Self::Internal => "internal error",
        })
    }
}
