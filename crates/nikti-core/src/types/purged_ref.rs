use serde::{Deserialize, Serialize};

use super::common::{TableId, Timestamp};

/// Resolution (confirmed 2026-07-26): a PurgedRef is a content tombstone for
/// a Table. Purging reclaims the Table's physical Parquet bytes only (e.g.
/// storage reclamation or a compliance-driven deletion request) — the
/// Table's id, metadata, and provenance record permanently remain in the
/// Table Repository, replaced by this marker. This is never a delete of the
/// Table object itself (INV-002 still holds: the Table's identity is never
/// removed), and PurgedRef itself is then immutable — no write method may
/// ever be added, matching Category A treatment for evidence that documents
/// what happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgedRef {
    table_id: TableId,
    content_fingerprint: String,
    purged_at: Timestamp,
    reason: String,
}

impl PurgedRef {
    pub fn new(table_id: TableId, content_fingerprint: String, purged_at: Timestamp, reason: String) -> Self {
        Self {
            table_id,
            content_fingerprint,
            purged_at,
            reason,
        }
    }

    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    pub fn content_fingerprint(&self) -> &str {
        &self.content_fingerprint
    }

    pub fn purged_at(&self) -> Timestamp {
        self.purged_at
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}
