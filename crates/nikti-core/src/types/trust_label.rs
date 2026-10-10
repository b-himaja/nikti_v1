use serde::{Deserialize, Serialize};

/// The closed trust-label vocabulary (Inspection Report Design Spec).
/// Every computed value the Inspection Engine emits carries exactly one of
/// these. `Inferred` is deliberately not a variant — Inspection never
/// infers anything. `Calculated` is deliberately not a variant either —
/// the spec replaces it with `Derived` for vocabulary consistency. Do not
/// add variants without updating the spec this mirrors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLabel {
    Measured,
    Derived,
    Detected,
    Metadata,
    FullData,
    Sampled,
    Pattern,
}
