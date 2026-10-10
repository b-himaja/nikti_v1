use crate::types::common::ProjectId;
use crate::types::Project;

use super::errors::RepositoryError;

/// Stores/organizes/retrieves Projects (INV-008: repositories manage,
/// workspaces execute). A Project's Datasets and Operation Log travel with
/// it as its mutable working state — see Project, Dataset (INV-003).
pub trait ProjectRepository {
    async fn insert(&self, project: Project) -> Result<(), RepositoryError>;
    async fn get(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError>;
    async fn list(&self) -> Result<Vec<Project>, RepositoryError>;

    /// Persists an updated Project (e.g. new Dataset added, new Operation
    /// Log entries appended). Legal because Project/Dataset are working
    /// objects, not evidence — contrast with Table/Report repositories,
    /// which have no such method for an existing item.
    async fn save(&self, project: Project) -> Result<(), RepositoryError>;

    async fn archive(&self, id: ProjectId) -> Result<(), RepositoryError>;
}

/// Test/dev in-memory reference implementation — same status as
/// `table_repository::testing::InMemoryTableRepository`: not the frozen
/// embedded-relational-store tech decision, but real (non-`todo!()`)
/// logic so command-layer code that needs a Project (e.g.
/// `commands::load_table_into_project`) can be exercised end to end.
pub mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct InMemoryProjectRepository {
        inner: Mutex<HashMap<ProjectId, Project>>,
    }

    impl InMemoryProjectRepository {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl ProjectRepository for InMemoryProjectRepository {
        async fn insert(&self, project: Project) -> Result<(), RepositoryError> {
            self.inner.lock().unwrap().insert(project.id(), project);
            Ok(())
        }

        async fn get(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError> {
            Ok(self.inner.lock().unwrap().get(&id).cloned())
        }

        async fn list(&self) -> Result<Vec<Project>, RepositoryError> {
            Ok(self.inner.lock().unwrap().values().cloned().collect())
        }

        async fn save(&self, project: Project) -> Result<(), RepositoryError> {
            let mut inner = self.inner.lock().unwrap();
            if !inner.contains_key(&project.id()) {
                return Err(RepositoryError::NotFound);
            }
            inner.insert(project.id(), project);
            Ok(())
        }

        async fn archive(&self, id: ProjectId) -> Result<(), RepositoryError> {
            // `Project` has no `archived` field yet (out of scope for this
            // task) — this only confirms the Project exists, matching
            // `RepositoryError::NotFound` semantics elsewhere, without
            // fabricating a state transition this type can't represent.
            if !self.inner.lock().unwrap().contains_key(&id) {
                return Err(RepositoryError::NotFound);
            }
            Ok(())
        }
    }
}
