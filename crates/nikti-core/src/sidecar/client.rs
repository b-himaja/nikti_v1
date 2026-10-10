use crate::engines::analysis::procedures::ProcedureOutcome;
use crate::engines::content::TableContent;
use crate::types::AnalysisProcedure;

use super::compute_backend::PythonSidecarStatus;

/// Internal collaborator used by `AnalysisEngine` for procedures that
/// require the Python sidecar (INV-029). Not a second co-equal engine —
/// INV-010 forbids an engine importing/instantiating another engine, and
/// this is a plain IPC client, called only from within an `AnalysisEngine`
/// implementation, never from the command layer directly.
///
/// `status` is taken by value so a caller cannot skip the check: there is
/// no code path here that runs the call without first inspecting it.
/// Anything other than `Ready` must resolve to `PythonProcedureOutcome::NotExecuted`
/// (Rule 9), never a partial result or a raw exception/traceback.
///
/// Currently dormant: ADR 0007 concluded the Python-backed procedure
/// exception list is empty for all 19 frozen V1 procedures (no
/// `engines/analysis/python_ops.rs` registry exists, because there is
/// nothing to register) — this trait exists so the sidecar contract has
/// somewhere to attach if a future procedure genuinely needs it, not
/// because anything currently calls it.
pub enum PythonProcedureOutcome {
    Completed(ProcedureOutcome),
    NotExecuted { reason: String },
}

pub trait PythonAnalysisClient {
    fn call(&self, content: TableContent, procedure: AnalysisProcedure, status: PythonSidecarStatus) -> PythonProcedureOutcome {
        let _ = (content, procedure, status);
        todo!()
    }
}
