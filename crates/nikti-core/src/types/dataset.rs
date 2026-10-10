use serde::{Deserialize, Serialize};

use super::common::{DatasetId, ProjectId, ReportId, TableId, Timestamp};
use super::table::TableSchema;

/// A rollback point captured before an operation was applied. Datasets are
/// working objects, not evidence (INV-003) — snapshots are workspace
/// convenience state, bounded and prunable, not subject to any
/// append-only/immutability contract.
///
/// Unlike the legacy prototype this was ported from, rolling back to a
/// snapshot does **not** delete or truncate any Project Operation Log
/// entry — INV-005 forbids that unconditionally, with no carve-out for
/// "undo". A rollback is itself recorded as a new appended log entry; the
/// entries for the operations it reverts remain in the log forever, same
/// as everything else that happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSnapshot {
    index: u32,
    content_ref: String,
    content_fingerprint: String,
    schema: TableSchema,
    row_count: u64,
    operation: String,
    note: Option<String>,
    created_at: Timestamp,
}

impl DatasetSnapshot {
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn content_ref(&self) -> &str {
        &self.content_ref
    }

    pub fn content_fingerprint(&self) -> &str {
        &self.content_fingerprint
    }

    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }

    pub fn row_count(&self) -> u64 {
        self.row_count
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

/// Matches the legacy prototype's `MAX_SNAPSHOTS`. Disk eviction for very
/// large datasets (the legacy `DISK_EVICT_ROWS` behavior) is a storage/
/// repository-layer concern now that content-addressed Parquet is wired up
/// (`content_ref`, below), not part of this in-memory domain type.
const MAX_SNAPSHOTS: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum DatasetError {
    #[error("snapshot {0} not found")]
    SnapshotNotFound(u32),
}

/// INV-003: the mutable working representation of data inside a Project.
/// Not evidence, not subject to INV-002/004 — freely created, modified, and
/// discarded within its owning Project. The Transformation Engine's API
/// accepts only Dataset identifiers as targets, never Table identifiers
/// (INV-013).
///
/// `source_dataset_id` and `source_table_id` are each an immediate-parent
/// pointer, not a flattened ancestor array — the full ordered lineage
/// chain (originating Table, then each parent Dataset in sequence) is
/// reconstructed by walking `source_dataset_id` back to a Dataset whose
/// `source_dataset_id` is `None`, at which point `source_table_id` (if
/// any) is the chain's root. Chosen over a flattened `Vec` because it
/// matches this codebase's existing provenance pattern (`ProvenanceRef`)
/// and never needs copying/growing on each derivation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    id: DatasetId,
    project_id: ProjectId,
    name: String,
    source_table_id: Option<TableId>,
    source_dataset_id: Option<DatasetId>,
    schema: TableSchema,
    row_count: u64,
    content_ref: String,
    content_fingerprint: String,
    report_refs: Vec<ReportId>,
    snapshots: Vec<DatasetSnapshot>,
    snapshot_counter: u32,
    updated_at: Timestamp,
}

impl Dataset {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: DatasetId,
        project_id: ProjectId,
        name: String,
        source_table_id: Option<TableId>,
        source_dataset_id: Option<DatasetId>,
        schema: TableSchema,
        row_count: u64,
        content_ref: String,
        content_fingerprint: String,
        updated_at: Timestamp,
    ) -> Self {
        Self {
            id,
            project_id,
            name,
            source_table_id,
            source_dataset_id,
            schema,
            row_count,
            content_ref,
            content_fingerprint,
            report_refs: Vec::new(),
            snapshots: Vec::new(),
            snapshot_counter: 0,
            updated_at,
        }
    }

    pub fn id(&self) -> DatasetId {
        self.id
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source_table_id(&self) -> Option<TableId> {
        self.source_table_id
    }

    pub fn source_dataset_id(&self) -> Option<DatasetId> {
        self.source_dataset_id
    }

    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }

    pub fn row_count(&self) -> u64 {
        self.row_count
    }

    pub fn content_ref(&self) -> &str {
        &self.content_ref
    }

    pub fn content_fingerprint(&self) -> &str {
        &self.content_fingerprint
    }

    pub fn report_refs(&self) -> &[ReportId] {
        &self.report_refs
    }

    pub fn add_report_ref(&mut self, report_id: ReportId) {
        self.report_refs.push(report_id);
    }

    pub fn snapshots(&self) -> &[DatasetSnapshot] {
        &self.snapshots
    }

    pub fn rename(&mut self, name: String, updated_at: Timestamp) {
        self.name = name;
        self.updated_at = updated_at;
    }

    /// Replaces the Dataset's working content after a Transformation
    /// Engine operation, snapshotting the prior state first (bounded to
    /// `MAX_SNAPSHOTS`, oldest evicted). Legal only because Dataset is a
    /// working object (INV-003) — the equivalent operation on a Table is
    /// forbidden under all circumstances (INV-002). The Project Operation
    /// Log append for this change is the command layer's responsibility,
    /// not this method's — see INV-013.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_change(
        &mut self,
        content_ref: String,
        content_fingerprint: String,
        schema: TableSchema,
        row_count: u64,
        operation: String,
        note: Option<String>,
        updated_at: Timestamp,
    ) {
        let snapshot = DatasetSnapshot {
            index: self.snapshot_counter,
            content_ref: self.content_ref.clone(),
            content_fingerprint: self.content_fingerprint.clone(),
            schema: self.schema.clone(),
            row_count: self.row_count,
            operation,
            note,
            created_at: self.updated_at,
        };
        self.snapshot_counter += 1;
        self.snapshots.push(snapshot);
        if self.snapshots.len() > MAX_SNAPSHOTS {
            self.snapshots.remove(0);
        }

        self.content_ref = content_ref;
        self.content_fingerprint = content_fingerprint;
        self.schema = schema;
        self.row_count = row_count;
        self.updated_at = updated_at;
    }

    /// Restores content from a prior snapshot. Snapshots newer than the
    /// target are dropped from the in-memory rollback buffer (they no
    /// longer represent a reachable future state) — this never touches
    /// the Project Operation Log, which keeps every entry forever
    /// regardless of rollback (INV-005). The command layer is
    /// responsible for appending a new log entry recording that this
    /// rollback happened.
    pub fn rollback_to(&mut self, snapshot_index: u32, updated_at: Timestamp) -> Result<(), DatasetError> {
        let position = self
            .snapshots
            .iter()
            .position(|s| s.index == snapshot_index)
            .ok_or(DatasetError::SnapshotNotFound(snapshot_index))?;

        let snapshot = self.snapshots[position].clone();
        self.content_ref = snapshot.content_ref;
        self.content_fingerprint = snapshot.content_fingerprint;
        self.schema = snapshot.schema;
        self.row_count = snapshot.row_count;
        self.updated_at = updated_at;
        self.snapshots.truncate(position);

        Ok(())
    }
}
