//! Did not exist before this task (verified: no `DatasetRepository` of any
//! kind was in this codebase). A Dataset is a Working Object (INV-003) —
//! this trait is deliberately update-permissive (`save` on an existing
//! item is legal), the opposite of `TableRepository`'s insert-only
//! contract, mirroring `ProjectRepository`'s own already-established
//! shape for the same reason.

use crate::types::common::{DatasetId, ProjectId};
use crate::types::Dataset;

use super::errors::RepositoryError;

pub trait DatasetRepository {
    async fn insert(&self, dataset: Dataset) -> Result<(), RepositoryError>;
    async fn get(&self, id: DatasetId) -> Result<Option<Dataset>, RepositoryError>;
    async fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<Dataset>, RepositoryError>;

    /// Persists an updated Dataset (new content after a transformation,
    /// a rename, a rollback, ...). Legal because Dataset is a working
    /// object, not evidence — contrast with `TableRepository`, which has
    /// no such method for an existing Table.
    async fn save(&self, dataset: Dataset) -> Result<(), RepositoryError>;
}

pub mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct InMemoryDatasetRepository {
        inner: Mutex<HashMap<DatasetId, Dataset>>,
    }

    impl InMemoryDatasetRepository {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl DatasetRepository for InMemoryDatasetRepository {
        async fn insert(&self, dataset: Dataset) -> Result<(), RepositoryError> {
            self.inner.lock().unwrap().insert(dataset.id(), dataset);
            Ok(())
        }

        async fn get(&self, id: DatasetId) -> Result<Option<Dataset>, RepositoryError> {
            Ok(self.inner.lock().unwrap().get(&id).cloned())
        }

        async fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<Dataset>, RepositoryError> {
            Ok(self.inner.lock().unwrap().values().filter(|d| d.project_id() == project_id).cloned().collect())
        }

        async fn save(&self, dataset: Dataset) -> Result<(), RepositoryError> {
            let mut inner = self.inner.lock().unwrap();
            if !inner.contains_key(&dataset.id()) {
                return Err(RepositoryError::NotFound);
            }
            inner.insert(dataset.id(), dataset);
            Ok(())
        }
    }
}
