//! Analysis's Python-backed procedure exception list (INV-029). An
//! explicit, reviewed, checked-in registry — never an ad-hoc runtime
//! choice. Currently empty: `docs/adr/0007-analysis-python-exception-
//! list-empty.md` records the investigation that concluded all 19
//! frozen Analysis procedures have a mature native Polars equivalent,
//! so none qualifies for the exception list.
//!
//! This file exists so the registry itself is a real, checked-in,
//! enumerable artifact — not merely a decision recorded in an ADR with
//! no corresponding code. An empty list is still an explicit list: no
//! Analysis procedure is routable to `ComputeBackend::Python` under any
//! circumstance today (`analyse_python`/`PythonAnalysisClient::call` are
//! separately still `todo!()` at the command/sidecar layer — an empty
//! exception list and an unimplemented dispatch path are two distinct
//! gaps, both real, neither hidden by the other). Adding an entry here
//! requires the same ADR-reviewed process that produced ADR 0007, not a
//! direct edit.
pub const PYTHON_BACKED_PROCEDURES: &[&str] = &[];
