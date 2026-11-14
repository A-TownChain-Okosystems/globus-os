use core::fmt;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Clone, PartialEq, Eq, Hash)]
        /// Typisierte Kennung; leere oder nur aus Leerzeichen bestehende Werte werden abgelehnt.
        pub struct $name(String);

        impl $name {
            /// Baut die Kennung; leerer Text ergibt `InvalidRequest`.
            pub fn new(value: impl Into<String>) -> Result<Self, crate::AuroraError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(crate::AuroraError::InvalidRequest);
                }
                Ok(Self(value))
            }

            /// Geliehener Text der Kennung.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

typed_id!(AgentId);
typed_id!(SkillId);
typed_id!(ToolId);
typed_id!(ModelId);
typed_id!(RequestId);
typed_id!(SessionId);
typed_id!(CapabilityId);
typed_id!(PolicyId);
typed_id!(ApprovalId);
typed_id!(EventId);

#[derive(Debug, Clone, PartialEq, Eq)]
/// Eine Anfrage an die Aurora-Runtime.
pub struct AuroraRequest {
    /// Kennung dieser Anfrage.
    pub request_id: RequestId,
    /// Sitzung, zu der die Anfrage gehoert.
    pub session_id: SessionId,
    /// Handelnder Akteur; darf nicht leer sein.
    pub principal: String,
    /// Was die Anfrage erreichen soll; darf nicht leer sein.
    pub intent: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ergebnis einer Anfrage.
pub struct AuroraResponse {
    /// Kennung der Anfrage, zu der das Ergebnis gehoert.
    pub request_id: RequestId,
    /// Status nach der Ausfuehrung.
    pub status: crate::RequestStatus,
    /// Ausgabe, sobald die Anfrage abgeschlossen ist.
    pub result: Option<String>,
    /// Verweis in den Audit-Trail, falls einer existiert.
    pub audit_reference: Option<String>,
}
