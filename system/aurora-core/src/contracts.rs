use crate::{
    AgentId, ApprovalId, ApprovalState, AuroraError, AuroraRequest, CapabilityId, Decision,
    ModelId, PolicyId, RequestStatus, RiskLevel, SkillId, ToolId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ergebnis der Pruefung einer Capability.
pub struct CapabilityResult {
    /// Entscheidung der Pruefung.
    pub decision: Decision,
    /// Begruendung, die dem Aufrufer gezeigt werden darf.
    pub reason: String,
    /// Policy, die die Entscheidung getragen hat.
    pub policy_id: Option<PolicyId>,
    /// Freigabe, auf der die Entscheidung beruht.
    pub approval_id: Option<ApprovalId>,
    /// Risikoklasse der Capability.
    pub risk: RiskLevel,
}

/// Capability, die vor dem Handeln geprueft wird.
pub trait Capability {
    /// Kennung der Capability.
    fn id(&self) -> &CapabilityId;
    /// Prueft die Capability fuer einen Akteur.
    fn evaluate(&self, principal: &str) -> CapabilityResult;
}

/// Policy, die ueber die Nutzung einer Capability entscheidet.
pub trait Policy {
    /// Kennung der Policy.
    fn id(&self) -> &PolicyId;
    /// Entscheidet ueber eine Capability fuer einen Akteur.
    fn evaluate(&self, capability: &CapabilityId, principal: &str) -> Decision;
}

/// Menschliche Freigabe vor der Ausfuehrung.
pub trait Approval {
    /// Legt eine Freigabeanfrage an.
    fn request(&self, reason: &str) -> Result<ApprovalId, AuroraError>;
    /// Zustand einer Freigabeanfrage.
    fn status(&self, approval_id: &ApprovalId) -> ApprovalState;
}

/// Agent, der eine Anfrage ausfuehrt.
pub trait Agent {
    /// Kennung des Agenten.
    fn id(&self) -> &AgentId;
    /// Fuehrt eine Anfrage aus.
    fn execute(&self, request: &AuroraRequest) -> Result<AgentResult, AuroraError>;
}

/// Benannte Faehigkeit, die ein Agent nutzen kann.
pub trait Skill {
    /// Kennung des Skills.
    fn id(&self) -> &SkillId;
}

/// Werkzeug, das die Runtime aufrufen kann.
pub trait Tool {
    /// Kennung des Werkzeugs.
    fn id(&self) -> &ToolId;
    /// Fuehrt einen Werkzeugaufruf aus.
    fn execute(&self, request: &ToolRequest) -> Result<ToolResult, AuroraError>;
}

/// Modell, das die Runtime abfragen kann.
pub trait Model {
    /// Kennung des Modells.
    fn id(&self) -> &ModelId;
    /// Fuehrt eine Inferenz aus.
    fn infer(&self, request: &ModelRequest) -> Result<ModelResult, AuroraError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ergebnis einer Agentenausfuehrung.
pub struct AgentResult {
    /// Status, den der Agent zurueckmeldet.
    pub status: RequestStatus,
    /// Ausgabe des Agenten.
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein Werkzeugaufruf.
pub struct ToolRequest {
    /// Aufgerufenes Werkzeug.
    pub tool_id: ToolId,
    /// Anfrage, in deren Rahmen das Werkzeug laeuft.
    pub request_id: crate::RequestId,
    /// Eingabe fuer das Werkzeug.
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ausgabe eines Werkzeugaufrufs.
pub struct ToolResult {
    /// Ausgabe des Werkzeugs.
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Eine Modellanfrage.
pub struct ModelRequest {
    /// Angefragtes Modell.
    pub model_id: ModelId,
    /// Anfrage, in deren Rahmen die Inferenz laeuft.
    pub request_id: crate::RequestId,
    /// Prompt an das Modell.
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ausgabe einer Modellanfrage.
pub struct ModelResult {
    /// Ausgabe des Modells.
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Auftrag an die Runtime.
pub struct ExecutionRequest {
    /// Anfrage, die ausgefuehrt werden soll.
    pub request_id: crate::RequestId,
    /// Capability, unter der ausgefuehrt wird.
    pub capability: CapabilityId,
    /// Eingabe fuer die Ausfuehrung.
    pub input: String,
}

/// Fuehrt eine Anfrage von Ende zu Ende aus.
pub trait Runtime {
    /// Fuehrt einen Auftrag aus.
    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult, AuroraError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ergebnis einer Runtime-Ausfuehrung.
pub struct ExecutionResult {
    /// Status nach der Ausfuehrung.
    pub status: RequestStatus,
    /// Ausgabe der Ausfuehrung.
    pub output: Option<String>,
}
