use serde::{Deserialize, Serialize};

use super::common::{LogEntryId, RecipeId, Timestamp};
use super::transformation_operation::TransformationOperation;

/// Knowledge, not evidence (INV-006): editable and versionable, unlike Table/
/// Report/log entries. INV-007: `source_operation_log_entry` is the only
/// legitimate origin — never a Table Event Log entry, Report, or Table.
///
/// `steps` is a sequence of the same `TransformationOperation` values a
/// single ad hoc Transformation call uses — a Recipe is just a named,
/// saved, replayable sequence of them, not a distinct kind of thing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    id: RecipeId,
    name: String,
    version: u32,
    steps: Vec<TransformationOperation>,
    source_operation_log_entry: LogEntryId,
    updated_at: Timestamp,
}

impl Recipe {
    /// INV-007: intended to be called only by promotion from a Project
    /// Operation Log entry — `source_operation_log_entry` must be one.
    pub fn new(
        id: RecipeId,
        name: String,
        steps: Vec<TransformationOperation>,
        source_operation_log_entry: LogEntryId,
        updated_at: Timestamp,
    ) -> Self {
        Self {
            id,
            name,
            version: 1,
            steps,
            source_operation_log_entry,
            updated_at,
        }
    }

    pub fn id(&self) -> RecipeId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn steps(&self) -> &[TransformationOperation] {
        &self.steps
    }

    pub fn source_operation_log_entry(&self) -> LogEntryId {
        self.source_operation_log_entry
    }

    /// INV-009: Recipes are authored/edited only inside a Project — the
    /// command layer, not this type, is responsible for enforcing that
    /// context before calling this.
    pub fn rename(&mut self, name: String, updated_at: Timestamp) {
        self.name = name;
        self.updated_at = updated_at;
    }

    pub fn set_steps(&mut self, steps: Vec<TransformationOperation>, updated_at: Timestamp) {
        self.steps = steps;
        self.version += 1;
        self.updated_at = updated_at;
    }
}
