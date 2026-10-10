use serde::{Deserialize, Serialize};

use super::common::{DatasetId, LogEntryId, ProjectId, RecipeId, Timestamp};
use super::transformation_operation::TransformationOperation;

/// INV-004/005 Category B: once appended, an entry is itself zero-mutation.
/// No `&mut self` method may ever be added to this struct.
///
/// INV-007: only entries in this log (never Table Event Log entries,
/// Reports, or Tables) are eligible sources for Recipe promotion.
///
/// `transformation_operation` is `Some` only for entries that actually
/// are a Transformation step (`run_transformation`/`apply_recipe_to_dataset`)
/// — `Recipe` promotion reads this field to reconstruct real, executable
/// `steps`. Entries for non-transformation Project actions (`load`,
/// `duplicate`, `run_validation`, `run_analysis`, ...) carry `None` here;
/// `operation` remains a human-readable label for every entry regardless.
///
/// `recipe_id`/`recipe_version` (Audit Report task, Part A): the actual
/// root-cause fix for a gap the Lineage Report task found and routed
/// around — `apply_recipe_to_dataset` previously forwarded straight into
/// `run_transformation` with no trace of which Recipe (if any) drove the
/// step, so a recipe-applied transformation and a manually-run one were
/// indistinguishable after the fact. Both `Some` only when this specific
/// step came from `apply_recipe_to_dataset`; `None` for a manual
/// `run_transformation` call, including one whose `operations` happen to
/// match a Recipe's steps by coincidence — this records *how the step
/// was actually invoked*, never inferred from its shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectOperationLogEntry {
    id: LogEntryId,
    project_id: ProjectId,
    dataset_id: DatasetId,
    recorded_at: Timestamp,
    operation: String,
    transformation_operation: Option<TransformationOperation>,
    recipe_id: Option<RecipeId>,
    recipe_version: Option<u32>,
}

impl ProjectOperationLogEntry {
    /// Constructs a new entry to hand to `ProjectOperationLog::append()`.
    /// Not a write path against an existing entry — there is no way to
    /// obtain a `&mut ProjectOperationLogEntry` for one already appended
    /// (see `entries()` below, which returns an immutable slice).
    pub fn new(
        id: LogEntryId,
        project_id: ProjectId,
        dataset_id: DatasetId,
        recorded_at: Timestamp,
        operation: String,
    ) -> Self {
        Self {
            id,
            project_id,
            dataset_id,
            recorded_at,
            operation,
            transformation_operation: None,
            recipe_id: None,
            recipe_version: None,
        }
    }

    /// Same as `new`, but carrying the real `TransformationOperation` this
    /// entry records — used by `run_transformation` (manual, no Recipe)
    /// so `promote_operation_log_to_recipe` has real, replayable steps to
    /// promote, not just a string label. `recipe_id`/`recipe_version` are
    /// always `None` here — see `new_recipe_transformation` for the
    /// Recipe-driven case.
    pub fn new_transformation(
        id: LogEntryId,
        project_id: ProjectId,
        dataset_id: DatasetId,
        recorded_at: Timestamp,
        operation: String,
        transformation_operation: TransformationOperation,
    ) -> Self {
        Self {
            id,
            project_id,
            dataset_id,
            recorded_at,
            operation,
            transformation_operation: Some(transformation_operation),
            recipe_id: None,
            recipe_version: None,
        }
    }

    /// Same as `new_transformation`, for the one case that's actually
    /// distinguishable at the point of invocation: `apply_recipe_to_dataset`
    /// knows which Recipe (and which specific version) it was given, so
    /// this constructor is the one real place that fact gets recorded.
    #[allow(clippy::too_many_arguments)]
    pub fn new_recipe_transformation(
        id: LogEntryId,
        project_id: ProjectId,
        dataset_id: DatasetId,
        recorded_at: Timestamp,
        operation: String,
        transformation_operation: TransformationOperation,
        recipe_id: RecipeId,
        recipe_version: u32,
    ) -> Self {
        Self {
            id,
            project_id,
            dataset_id,
            recorded_at,
            operation,
            transformation_operation: Some(transformation_operation),
            recipe_id: Some(recipe_id),
            recipe_version: Some(recipe_version),
        }
    }

    pub fn transformation_operation(&self) -> Option<&TransformationOperation> {
        self.transformation_operation.as_ref()
    }

    pub fn recipe_id(&self) -> Option<RecipeId> {
        self.recipe_id
    }

    pub fn recipe_version(&self) -> Option<u32> {
        self.recipe_version
    }

    pub fn id(&self) -> LogEntryId {
        self.id
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn dataset_id(&self) -> DatasetId {
        self.dataset_id
    }

    pub fn recorded_at(&self) -> Timestamp {
        self.recorded_at
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }
}

/// INV-004/005 Category B: the only legal write operation is `append()`.
/// No update/delete/clear/truncate/reorder method may ever be added here —
/// a correction is a new appended entry, never an edit of an existing one.
/// (Read-only accessors below are not writes and are not restricted by this.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectOperationLog {
    entries: Vec<ProjectOperationLogEntry>,
}

impl ProjectOperationLog {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn append(&mut self, entry: ProjectOperationLogEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[ProjectOperationLogEntry] {
        &self.entries
    }
}
