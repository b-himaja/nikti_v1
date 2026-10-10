//! Transformation's Python-backed operation exception list (INV-029).
//! An explicit, reviewed, checked-in registry — never an ad-hoc runtime
//! choice. Currently empty: `docs/adr/0006-transformation-python-
//! exception-list-empty.md` records the investigation that concluded
//! every ported Transformation operation has a mature native Polars
//! equivalent, so none qualifies for the exception list (INV-029: an
//! operation qualifies only if it has no mature native equivalent — not
//! "an equivalent exists but is more code to write").
//!
//! This file exists so the registry itself is a real, checked-in,
//! enumerable artifact — not merely a decision recorded in an ADR with
//! no corresponding code. An empty list is still an explicit list: no
//! Transformation operation is routable to `ComputeBackend::Python`
//! under any circumstance today, and `run_transformation`
//! (`commands/project_workspace.rs`) has no `ComputeBackend` parameter
//! at all yet — routing to a Python backend for Transformation is not
//! just unpopulated, it isn't wired at the command layer either. Adding
//! an entry here requires the same ADR-reviewed process that produced
//! ADR 0006, not a direct edit.
pub const PYTHON_BACKED_OPERATIONS: &[&str] = &[];
