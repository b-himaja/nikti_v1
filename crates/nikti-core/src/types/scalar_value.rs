use polars::prelude::*;

/// A parameter-level literal value — the Rust equivalent of the loose
/// JSON scalars the legacy payload dicts accepted (constant fill values,
/// recode mapping targets, conditional-flag `then`/`default` values).
/// Deliberately closed (no arbitrary Python-style duck typing) — every
/// call site must pick one of these variants explicitly.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScalarValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl ScalarValue {
    /// Builds a `pl.lit(...)`-equivalent expression.
    pub fn to_lit_expr(&self) -> Expr {
        match self {
            ScalarValue::Null => lit(NULL),
            ScalarValue::Bool(b) => lit(*b),
            ScalarValue::Int(i) => lit(*i),
            ScalarValue::Float(f) => lit(*f),
            ScalarValue::Str(s) => lit(s.clone()),
        }
    }
}
