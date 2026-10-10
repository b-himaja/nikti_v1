use crate::types::common::TableId;
use crate::types::{AdminState, DescriptiveMetadata, PurgedRef, Table, TableEventLog, TableEventLogEntry, TableRegistrationError, TableRegistrationOutcome, TableRegistrationRequest};

use super::errors::RepositoryError;

/// INV-002/004: the collection is insert-only for *new* Tables — that
/// permission never extends to an already-existing one. There is
/// deliberately no `update`/`delete`/`patch` method that replaces an
/// existing Table's *content* (`schema_ref`, `table_data_ref`,
/// `creation_source`, `source_provenance`, `version_info`), and none may
/// ever be added. `set_admin_state`/`set_descriptive_metadata` are a
/// narrow, explicitly-scoped exception: they only ever replace the
/// lifecycle/display sub-objects (see `Table`'s doc comment for why this
/// doesn't violate INV-002) — never row/column/cell/schema content. The
/// only content-affecting operation on an existing Table's physical bytes
/// is `purge`, which records a `PurgedRef` tombstone and never touches the
/// Table's row/metadata identity (see `PurgedRef` docs).
pub trait TableRepository {
    /// Amendment 10: idempotency-aware registration, keyed on
    /// `idempotency_key` and checked against the (`data_fingerprint`,
    /// `schema_fingerprint`) *pair* on `request.table_data_ref`/
    /// `request.schema_ref` — never `data_fingerprint` alone. Inserts the
    /// Table at `TableState::Registered`; the caller is responsible for
    /// the Amendment 7 log-then-available follow-on (`append_event` then
    /// `set_admin_state`).
    async fn register_table(&self, request: TableRegistrationRequest) -> Result<TableRegistrationOutcome, TableRegistrationError>;

    async fn get(&self, id: TableId) -> Result<Option<Table>, RepositoryError>;

    async fn list(&self) -> Result<Vec<Table>, RepositoryError>;

    /// Category B write path: append-only (INV-004/005).
    async fn append_event(&self, table_id: TableId, entry: TableEventLogEntry) -> Result<(), RepositoryError>;

    async fn event_log(&self, table_id: TableId) -> Result<TableEventLog, RepositoryError>;

    /// Amendment 7: replaces ONLY `admin_state` (`table_state`,
    /// `table_event_log_id`, `archived_at`) for an already-registered
    /// Table — used to transition Registered -> Available/Unavailable
    /// (gated on a Table Event Log entry existing) and for Archive/Restore
    /// (Part H). Never touches row/column/cell/schema content.
    async fn set_admin_state(&self, table_id: TableId, admin_state: AdminState) -> Result<(), RepositoryError>;

    /// Part A: replaces ONLY `descriptive_metadata` (`table_name`,
    /// `data_dictionary_id`) — renaming never touches `table_id`,
    /// fingerprints, `content_ref`, or `schema_ref`.
    async fn set_descriptive_metadata(&self, table_id: TableId, descriptive_metadata: DescriptiveMetadata) -> Result<(), RepositoryError>;

    /// Records a content tombstone for a Table (see PurgedRef). Never
    /// removes the Table's row/identity from this repository.
    async fn purge(&self, purged_ref: PurgedRef) -> Result<(), RepositoryError>;

    async fn get_purged_ref(&self, table_id: TableId) -> Result<Option<PurgedRef>, RepositoryError>;
}

/// Test/dev in-memory reference implementation — same status as
/// `report_repository::testing::InMemoryReportRepository`: not the frozen
/// embedded-relational-store tech decision, but real (non-`todo!()`)
/// logic so Amendment 7/10's stateful behavior is genuinely exercised.
pub mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Inner {
        tables: HashMap<TableId, Table>,
        idempotency_index: HashMap<String, (TableId, String, String)>,
        event_logs: HashMap<TableId, TableEventLog>,
        purged: HashMap<TableId, PurgedRef>,
    }

    #[derive(Default)]
    pub struct InMemoryTableRepository {
        inner: Mutex<Inner>,
        fail_event_append: bool,
    }

    impl InMemoryTableRepository {
        pub fn new() -> Self {
            Self::default()
        }

        /// Simulates a Table Event Log append failure (sidecar-independent
        /// storage failure, e.g. disk full) so Amendment 7's ordering
        /// guarantee — a Table never reaches `Available` without a
        /// `table_event_log_id` — can be exercised against real logic
        /// rather than asserted only in prose.
        pub fn with_event_append_failure() -> Self {
            Self { fail_event_append: true, ..Self::default() }
        }
    }

    impl TableRepository for InMemoryTableRepository {
        async fn register_table(&self, request: TableRegistrationRequest) -> Result<TableRegistrationOutcome, TableRegistrationError> {
            let mut inner = self.inner.lock().unwrap();

            let data_fp = request.table_data_ref.data_fingerprint.clone();
            let schema_fp = request.schema_ref.schema_fingerprint.clone();

            if let Some((existing_id, existing_data_fp, existing_schema_fp)) = inner.idempotency_index.get(&request.idempotency_key).cloned() {
                return if existing_data_fp == data_fp && existing_schema_fp == schema_fp {
                    Ok(TableRegistrationOutcome::Idempotent { table_id: existing_id })
                } else {
                    Err(TableRegistrationError::FingerprintPairConflict(request.idempotency_key.clone()))
                };
            }

            let table = Table::new(
                request.table_id,
                request.creation_source,
                request.source_provenance,
                request.table_data_ref,
                request.schema_ref,
                request.version_info,
                request.descriptive_metadata,
                AdminState::registered(),
                request.created_at,
                request.created_by,
            );

            inner.idempotency_index.insert(request.idempotency_key.clone(), (request.table_id, data_fp, schema_fp));
            inner.tables.insert(request.table_id, table);
            inner.event_logs.insert(request.table_id, TableEventLog::new());

            Ok(TableRegistrationOutcome::Registered { table_id: request.table_id })
        }

        async fn get(&self, id: TableId) -> Result<Option<Table>, RepositoryError> {
            Ok(self.inner.lock().unwrap().tables.get(&id).cloned())
        }

        async fn list(&self) -> Result<Vec<Table>, RepositoryError> {
            Ok(self.inner.lock().unwrap().tables.values().cloned().collect())
        }

        async fn append_event(&self, table_id: TableId, entry: TableEventLogEntry) -> Result<(), RepositoryError> {
            if self.fail_event_append {
                return Err(RepositoryError::Backend("table event log append failed".to_string()));
            }
            let mut inner = self.inner.lock().unwrap();
            inner.event_logs.entry(table_id).or_default().append(entry);
            Ok(())
        }

        async fn event_log(&self, table_id: TableId) -> Result<TableEventLog, RepositoryError> {
            Ok(self.inner.lock().unwrap().event_logs.get(&table_id).cloned().unwrap_or_default())
        }

        async fn set_admin_state(&self, table_id: TableId, admin_state: AdminState) -> Result<(), RepositoryError> {
            let mut inner = self.inner.lock().unwrap();
            let updated = inner.tables.get(&table_id).ok_or(RepositoryError::NotFound)?.with_admin_state(admin_state);
            inner.tables.insert(table_id, updated);
            Ok(())
        }

        async fn set_descriptive_metadata(&self, table_id: TableId, descriptive_metadata: DescriptiveMetadata) -> Result<(), RepositoryError> {
            let mut inner = self.inner.lock().unwrap();
            let updated = inner.tables.get(&table_id).ok_or(RepositoryError::NotFound)?.with_descriptive_metadata(descriptive_metadata);
            inner.tables.insert(table_id, updated);
            Ok(())
        }

        async fn purge(&self, purged_ref: PurgedRef) -> Result<(), RepositoryError> {
            self.inner.lock().unwrap().purged.insert(purged_ref.table_id(), purged_ref);
            Ok(())
        }

        async fn get_purged_ref(&self, table_id: TableId) -> Result<Option<PurgedRef>, RepositoryError> {
            Ok(self.inner.lock().unwrap().purged.get(&table_id).cloned())
        }
    }
}
