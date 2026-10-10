use serde::{Deserialize, Serialize};

use super::common::{DatasetId, ProjectId, Timestamp};
use super::project_operation_log::ProjectOperationLog;

/// A Project is the authoring/execution context for Datasets and Recipes
/// (INV-003/INV-009). Not an evidence object itself, but it owns the
/// Project Operation Log, which is (see ProjectOperationLog).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    id: ProjectId,
    name: String,
    created_at: Timestamp,
    dataset_ids: Vec<DatasetId>,
    operation_log: ProjectOperationLog,
}

impl Project {
    pub fn new(id: ProjectId, name: String, created_at: Timestamp) -> Self {
        Self {
            id,
            name,
            created_at,
            dataset_ids: Vec::new(),
            operation_log: ProjectOperationLog::new(),
        }
    }

    pub fn id(&self) -> ProjectId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn dataset_ids(&self) -> &[DatasetId] {
        &self.dataset_ids
    }

    pub fn operation_log(&self) -> &ProjectOperationLog {
        &self.operation_log
    }

    pub fn operation_log_mut(&mut self) -> &mut ProjectOperationLog {
        &mut self.operation_log
    }

    pub fn add_dataset(&mut self, dataset_id: DatasetId) {
        self.dataset_ids.push(dataset_id);
    }

    pub fn remove_dataset(&mut self, dataset_id: DatasetId) {
        self.dataset_ids.retain(|id| *id != dataset_id);
    }
}
