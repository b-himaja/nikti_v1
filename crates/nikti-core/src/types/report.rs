use serde::{Deserialize, Serialize};

use super::common::{EngineVersion, ProvenanceRef, ReportId, Timestamp};

pub use super::common::ReportKind;

/// Shared, read-only metadata embedded in every Report subtype.
///
/// INV-004 / Rule 5 Category A: fully immutable once created. Only read
/// accessors exist below — no subtype may add an update/delete/regenerate
/// method, and this struct must never grow one either.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMeta {
    id: ReportId,
    kind: ReportKind,
    generated_at: Timestamp,
    engine_version: EngineVersion,
    provenance: ProvenanceRef,
}

impl ReportMeta {
    /// INV-015/029: intended to be called only from within
    /// `ReportEngine::build_report()` — not a write path against an
    /// existing report, but the one legitimate way a new one comes into
    /// being.
    pub fn new(
        id: ReportId,
        kind: ReportKind,
        generated_at: Timestamp,
        engine_version: EngineVersion,
        provenance: ProvenanceRef,
    ) -> Self {
        Self {
            id,
            kind,
            generated_at,
            engine_version,
            provenance,
        }
    }

    pub fn id(&self) -> ReportId {
        self.id
    }

    pub fn kind(&self) -> ReportKind {
        self.kind
    }

    pub fn generated_at(&self) -> Timestamp {
        self.generated_at
    }

    pub fn engine_version(&self) -> &EngineVersion {
        &self.engine_version
    }

    pub fn provenance(&self) -> &ProvenanceRef {
        &self.provenance
    }
}

/// A read-only view common to all Report subtypes. INV-015: only
/// ReportEngine::build_report() constructs values behind this trait — no
/// other engine, repository, or UI component may implement or construct one.
///
/// This trait must never gain a write method (no `set_*`, no `regenerate`,
/// no `&mut self` of any kind) — see Rule 5 Category A and INV-004.
pub trait Report {
    fn meta(&self) -> &ReportMeta;
}
