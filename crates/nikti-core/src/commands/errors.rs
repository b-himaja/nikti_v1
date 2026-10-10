use thiserror::Error;
use uuid::Uuid;

use crate::commands::table_workspace::TableWorkspaceError;
use crate::engines::{ReportEngineError, TransformError};
use crate::repositories::RepositoryError;
use crate::types::report_registration::RegistrationError;
use crate::types::{AnalysisVersionState, DatasetError, VersionState};

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Report(#[from] ReportEngineError),
    #[error(transparent)]
    Transform(#[from] TransformError),
    #[error(transparent)]
    Dataset(#[from] DatasetError),
    #[error(transparent)]
    TableWorkspace(#[from] TableWorkspaceError),
    #[error(transparent)]
    ReportRegistration(#[from] RegistrationError),
    /// INV-007: Recipe promotion accepts only a Project Operation Log
    /// reference; any other source type is rejected with this.
    #[error("recipe promotion source must be a Project Operation Log entry")]
    InvalidRecipeSource,
    /// Execution-time gate for Validation: `version_state` must be
    /// `Active` for a version to run — a bare `ruleset.version(id)`
    /// lookup alone does not imply this (see `definition_diagnostics`
    /// ADR 0008's acknowledged gap).
    #[error("validation ruleset version {version_id} cannot run: state is {state:?}, must be Active")]
    ValidationVersionNotActive { version_id: Uuid, state: VersionState },
    /// Execution-time gate for Validation: `definition_status` must be
    /// `Valid` even on an `Active` version, since `version_state ==
    /// Active` does not structurally guarantee it (no post-deserialize
    /// validation hook exists on this type).
    #[error("validation ruleset version {version_id} cannot run: definition status is Invalid")]
    ValidationVersionDefinitionInvalid { version_id: Uuid },
    /// Execution-time gate for Analysis: same reasoning as
    /// `ValidationVersionNotActive`.
    #[error("analysis ruleset version {version_id} cannot run: state is {state:?}, must be Active")]
    AnalysisVersionNotActive { version_id: Uuid, state: AnalysisVersionState },
    /// Execution-time gate for Analysis: same reasoning as
    /// `ValidationVersionDefinitionInvalid`.
    #[error("analysis ruleset version {version_id} cannot run: definition status is Invalid")]
    AnalysisVersionDefinitionInvalid { version_id: Uuid },
}
