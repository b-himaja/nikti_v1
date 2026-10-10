//! Storage for `PendingTransformationPlan` (see that type's doc comment —
//! a newly-inferred concept for this task, not pre-existing). Same
//! Working Object update-permissive shape as `DatasetRepository`.

use uuid::Uuid;

use crate::types::common::ProjectId;
use crate::types::PendingTransformationPlan;

use super::errors::RepositoryError;

pub trait PendingPlanRepository {
    async fn save(&self, plan: PendingTransformationPlan) -> Result<(), RepositoryError>;
    async fn get(&self, id: Uuid) -> Result<Option<PendingTransformationPlan>, RepositoryError>;
    async fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<PendingTransformationPlan>, RepositoryError>;
}

pub mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct InMemoryPendingPlanRepository {
        inner: Mutex<HashMap<Uuid, PendingTransformationPlan>>,
    }

    impl InMemoryPendingPlanRepository {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl PendingPlanRepository for InMemoryPendingPlanRepository {
        async fn save(&self, plan: PendingTransformationPlan) -> Result<(), RepositoryError> {
            self.inner.lock().unwrap().insert(plan.id(), plan);
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<PendingTransformationPlan>, RepositoryError> {
            Ok(self.inner.lock().unwrap().get(&id).cloned())
        }

        async fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<PendingTransformationPlan>, RepositoryError> {
            Ok(self.inner.lock().unwrap().values().filter(|p| p.project_id() == project_id).cloned().collect())
        }
    }
}
