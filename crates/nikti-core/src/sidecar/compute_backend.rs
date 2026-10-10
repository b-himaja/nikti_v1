use serde::{Deserialize, Serialize};

/// INV-030: the sidecar's environment hash, computed once at sidecar
/// startup from the pinned interpreter + dependency lockfile. Carried by
/// `Ready` so a report built while the sidecar is up always has it at hand
/// for `EngineVersion::python_env_hash` — reproducibility claims are never
/// made against an unpinned or silently-drifting environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PythonSidecarStatus {
    Ready { python_env_hash: String },
    Unavailable { reason: String },
}

/// INV-029: a Python-backed Analysis procedure conforms to the exact same
/// `(content, definition) -> result` contract as a native Rust engine — this
/// enum is what distinguishes the two at the call site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputeBackend {
    Native,
    Python { status: PythonSidecarStatus },
}

/// INV-029: the Python sidecar's availability is a first-class field here,
/// not a hidden flag. Only the Analysis engine has a variant that can carry
/// a Python-backed `ComputeBackend` — every other engine has exactly one
/// backend (native Rust/Polars) and no field for it to vary, so a procedure
/// requiring the sidecar is structurally un-runnable (not merely
/// hidden/greyed-out by convention) whenever `status` is not `Ready`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineContext {
    Inspection,
    Validation,
    Analysis { backend: ComputeBackend },
    Transformation,
    Report,
}
