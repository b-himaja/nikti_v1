//! Did not exist anywhere in this codebase before this task (verified: no
//! "pending"/"PendingTransformation" match anywhere prior). The Project
//! Workspace task prompt named `save_pending_plan`/`load_pending_plan`/
//! `run_pending_plan` without this concept ever having been designed —
//! this is the minimal, reasonable reading: a named, saved, ordered list
//! of `TransformationOperation`s against one Dataset, not yet applied.
//! Low-risk to have inferred, since it's a Working Object (INV-003), same
//! class as `Dataset`/`Project` — freely created/edited/discarded, no
//! Category-A immutability test applies, and getting the shape wrong
//! costs a later revision, not an evidence-integrity violation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{DatasetId, ProjectId, Timestamp};
use super::transformation_operation::TransformationOperation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransformationPlan {
    id: Uuid,
    project_id: ProjectId,
    dataset_id: DatasetId,
    name: String,
    operations: Vec<TransformationOperation>,
    created_at: Timestamp,
}

impl PendingTransformationPlan {
    pub fn new(id: Uuid, project_id: ProjectId, dataset_id: DatasetId, name: String, operations: Vec<TransformationOperation>, created_at: Timestamp) -> Self {
        Self { id, project_id, dataset_id, name, operations, created_at }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn dataset_id(&self) -> DatasetId {
        self.dataset_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn operations(&self) -> &[TransformationOperation] {
        &self.operations
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}
