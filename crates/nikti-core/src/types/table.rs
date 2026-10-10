//! Table Object Model v1.0 (per the Table Repository Workspace task's own
//! summary — the source document does not exist in this repo). Part C:
//! the canonical stored Table Object is nested (`creation_source`,
//! `source_provenance`, `table_data_ref`, `schema_ref`, `version_info`,
//! `descriptive_metadata`, `admin_state`). Flattened/indexed fields for
//! Navigator list rows are always *derived* from this nested shape, never
//! a second competing model — see `repositories::table_repository`'s list
//! index for where that derivation happens.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{DatasetId, LogEntryId, ProjectId, TableId, Timestamp};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub columns: Vec<ColumnDef>,
}

/// Part A: two distinct fields — never conflate them. A Table exported
/// from a Project Dataset has `creation_source_type = ProjectDatasetExport`
/// and `source_type = ProjectDataset`, not the other way around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreationSourceType {
    Import,
    ProjectDatasetExport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    Csv,
    Excel,
    DatabaseTable,
    DatabaseQuery,
    Parquet,
    Json,
    ProjectDataset,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreationSource {
    pub creation_source_type: CreationSourceType,
    pub source_type: SourceType,
}

/// Where the content came from — a file path/URI for imports, or the
/// originating Project/Dataset for a `project_dataset_export`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub source_label: String,
    pub source_project_id: Option<ProjectId>,
    pub source_dataset_id: Option<DatasetId>,
}

/// Amendment 10: idempotency is checked on the (`data_fingerprint`,
/// `schema_fingerprint`) *pair* — never `data_fingerprint` alone. Kept as
/// two distinct fields for exactly that reason: a retried registration
/// with the same data but a different schema must never be silently
/// treated as the same content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDataRef {
    pub content_ref: String,
    pub data_fingerprint: String,
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaRef {
    pub schema: TableSchema,
    pub schema_fingerprint: String,
}

/// `version_family_id` groups related versions but is never itself a
/// loadable/executable reference — a Project always references one
/// specific `table_id`, never "the latest in this family" (Part A).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version_family_id: Uuid,
}

/// `table_name` is display metadata, not identity (Part A) — renaming
/// must never touch `table_id`, fingerprints, `content_ref`, `schema_ref`,
/// or any Project reference. `data_dictionary_id` is an opaque reference
/// to the (separate, future) Data Dictionary Service — Part D: a missing
/// dictionary disables only the `Open Data Dictionary` action and never
/// affects `table_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptiveMetadata {
    pub table_name: String,
    pub data_dictionary_id: Option<Uuid>,
}

/// Amendment 7: a Table must not reach `Available` until it has a
/// `table_event_log_id` (the id of the Table Event Log entry recorded for
/// its creation). If that append ever fails, the Table stays `Registered`
/// or becomes `Unavailable` — never `Available` without a log reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableState {
    Registered,
    Available,
    Archived,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminState {
    pub table_state: TableState,
    pub table_event_log_id: Option<LogEntryId>,
    pub archived_at: Option<Timestamp>,
}

impl AdminState {
    pub fn registered() -> Self {
        Self { table_state: TableState::Registered, table_event_log_id: None, archived_at: None }
    }
}

/// INV-002 / Rule 5 Category A: zero mutation after creation, ever.
///
/// This struct intentionally exposes ONLY read accessors, plus two
/// narrow, non-mutating "replace" constructors (`with_admin_state`,
/// `with_descriptive_metadata`) that return a brand-new `Table` value —
/// mirroring the precedent already established for `Report`'s
/// `with_state`. Neither ever takes `&mut self`; neither ever touches
/// `id`, `schema_ref`, `table_data_ref`, `creation_source`,
/// `source_provenance`, or `version_info` — only the repository decides
/// whether to replace which value is associated with a `table_id` in its
/// own collection, and it never mutates a value already handed to a
/// caller. `table_state`/`table_name` are lifecycle and display metadata,
/// not row/column/cell/schema content, so this is the same distinction
/// INV-002's code implication draws when it says a Table has no write
/// path against "its own rows, columns, or cells" — not against every
/// field on the struct being forever frozen at the byte level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    id: TableId,
    creation_source: CreationSource,
    source_provenance: SourceProvenance,
    table_data_ref: TableDataRef,
    schema_ref: SchemaRef,
    version_info: VersionInfo,
    descriptive_metadata: DescriptiveMetadata,
    admin_state: AdminState,
    created_at: Timestamp,
    created_by: String,
}

impl Table {
    /// Constructs a brand-new Table. Not a write path against an existing
    /// one — INV-002 forbids mutating an already-existing Table, not the
    /// creation of a new, independent one (e.g. by import, or as a
    /// correction with a new id).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: TableId,
        creation_source: CreationSource,
        source_provenance: SourceProvenance,
        table_data_ref: TableDataRef,
        schema_ref: SchemaRef,
        version_info: VersionInfo,
        descriptive_metadata: DescriptiveMetadata,
        admin_state: AdminState,
        created_at: Timestamp,
        created_by: String,
    ) -> Self {
        Self {
            id,
            creation_source,
            source_provenance,
            table_data_ref,
            schema_ref,
            version_info,
            descriptive_metadata,
            admin_state,
            created_at,
            created_by,
        }
    }

    pub fn id(&self) -> TableId {
        self.id
    }

    pub fn creation_source(&self) -> &CreationSource {
        &self.creation_source
    }

    pub fn source_provenance(&self) -> &SourceProvenance {
        &self.source_provenance
    }

    pub fn table_data_ref(&self) -> &TableDataRef {
        &self.table_data_ref
    }

    pub fn schema_ref(&self) -> &SchemaRef {
        &self.schema_ref
    }

    pub fn version_info(&self) -> &VersionInfo {
        &self.version_info
    }

    pub fn descriptive_metadata(&self) -> &DescriptiveMetadata {
        &self.descriptive_metadata
    }

    pub fn admin_state(&self) -> &AdminState {
        &self.admin_state
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn created_by(&self) -> &str {
        &self.created_by
    }

    // --- Flattened convenience accessors (Part C: derived from the
    // canonical nested object, never a second model) ---

    pub fn schema(&self) -> &TableSchema {
        &self.schema_ref.schema
    }

    pub fn row_count(&self) -> u64 {
        self.table_data_ref.row_count
    }

    pub fn table_state(&self) -> TableState {
        self.admin_state.table_state
    }

    pub fn table_name(&self) -> &str {
        &self.descriptive_metadata.table_name
    }

    /// Returns a new Table value with only `admin_state` replaced —
    /// never mutates `self`. See the struct-level doc comment for why
    /// this doesn't violate INV-002.
    pub fn with_admin_state(&self, admin_state: AdminState) -> Table {
        Table { admin_state, ..self.clone() }
    }

    /// Returns a new Table value with only `descriptive_metadata`
    /// replaced — `id`, fingerprints, `content_ref`, and `schema_ref` are
    /// untouched by construction (Part A). See the struct-level doc
    /// comment for why this doesn't violate INV-002.
    pub fn with_descriptive_metadata(&self, descriptive_metadata: DescriptiveMetadata) -> Table {
        Table { descriptive_metadata, ..self.clone() }
    }
}
