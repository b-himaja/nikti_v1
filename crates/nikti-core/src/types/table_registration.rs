//! Amendment 10: Table registration idempotency is checked on the
//! (`data_fingerprint`, `schema_fingerprint`) *pair*, never
//! `data_fingerprint` alone. Mirrors `report_registration`'s shape.

use super::common::TableId;
use super::table::{CreationSource, DescriptiveMetadata, SchemaRef, SourceProvenance, TableDataRef, VersionInfo};

#[derive(Debug, Clone)]
pub struct TableRegistrationRequest {
    pub idempotency_key: String,
    pub table_id: TableId,
    pub creation_source: CreationSource,
    pub source_provenance: SourceProvenance,
    pub table_data_ref: TableDataRef,
    pub schema_ref: SchemaRef,
    pub version_info: VersionInfo,
    pub descriptive_metadata: DescriptiveMetadata,
    pub created_at: super::common::Timestamp,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableRegistrationOutcome {
    /// A genuinely new Table was inserted, at `TableState::Registered`
    /// with `admin_state` per `AdminState::registered()` — the caller
    /// (the import command) is responsible for the Amendment 7
    /// log-then-available follow-on steps; this call never assigns
    /// `Available` itself.
    Registered { table_id: TableId },
    /// Same `idempotency_key` and same (`data_fingerprint`,
    /// `schema_fingerprint`) pair as an existing Table — the existing
    /// `table_id` is returned, no new Table is created.
    Idempotent { table_id: TableId },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableRegistrationError {
    /// Amendment 10: same `idempotency_key`, but a different
    /// `data_fingerprint` and/or `schema_fingerprint` — rejected outright,
    /// never silently treated as a duplicate of the existing Table.
    #[error("idempotency_key '{0}' already used with a different data_fingerprint/schema_fingerprint pair")]
    FingerprintPairConflict(String),
}
