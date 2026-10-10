use serde::{Deserialize, Serialize};

use super::common::{LogEntryId, TableId, Timestamp};

/// INV-004/005 Category B: once appended, an entry is itself zero-mutation.
/// No `&mut self` method may ever be added to this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableEventLogEntry {
    id: LogEntryId,
    table_id: TableId,
    recorded_at: Timestamp,
    event: String,
}

impl TableEventLogEntry {
    /// Constructs a new entry to hand to `TableEventLog::append()`. Not a
    /// write path against an existing entry — there is no way to obtain a
    /// `&mut TableEventLogEntry` for one already appended (see `entries()`
    /// below, which returns an immutable slice).
    pub fn new(id: LogEntryId, table_id: TableId, recorded_at: Timestamp, event: String) -> Self {
        Self {
            id,
            table_id,
            recorded_at,
            event,
        }
    }

    pub fn id(&self) -> LogEntryId {
        self.id
    }

    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    pub fn recorded_at(&self) -> Timestamp {
        self.recorded_at
    }

    pub fn event(&self) -> &str {
        &self.event
    }
}

/// INV-004/005 Category B: the only legal write operation is `append()`.
/// No update/delete/clear/truncate/reorder method may ever be added here —
/// a correction is a new appended entry, never an edit of an existing one.
/// (Read-only accessors below are not writes and are not restricted by this.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableEventLog {
    entries: Vec<TableEventLogEntry>,
}

impl TableEventLog {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn append(&mut self, entry: TableEventLogEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[TableEventLogEntry] {
        &self.entries
    }
}
