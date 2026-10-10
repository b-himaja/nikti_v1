//! Native port of `legacy/datawrangler/python/engine/ops/column_ops.py`.
//! Every function takes a `LazyFrame` and returns `TransformationResults`
//! via `super::finish`, per the "port to LazyFrame" instruction — a few
//! (discretise/scaler/flag_values) still need one intermediate `.collect()`
//! to pull out scalar statistics (bin edges, mean/std/thresholds) before
//! the final expression, exactly like the legacy code did in eager mode;
//! that's flagged inline, not hidden.

use polars::prelude::*;
use polars::series::ops::NullBehavior;

use crate::types::{
    DatetimeOpKind, DiscretiseMethod, FillStrategy, FlagMethod, HashAlgorithm, PadSide, ScalarValue,
    ScalerMethod, StringOpKind, TargetType,
};

use super::errors::TransformError;
use super::finish;
use super::formula::FormulaParser;

fn schema_of(lf: &LazyFrame) -> Result<SchemaRef, TransformError> {
    Ok(lf.clone().collect_schema()?)
}

fn require_column(schema: &Schema, name: &str) -> Result<(), TransformError> {
    if schema.get(name).is_none() {
        return Err(TransformError::ColumnNotFound(name.to_string()));
    }
    Ok(())
}

// ── 1. Numerical derivation using formula ───────────────────────────────

pub fn derive_column(
    lf: LazyFrame,
    output_col: &str,
    formula: &str,
    overwrite: bool,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    if !overwrite && schema.get(output_col).is_some() {
        return Err(TransformError::InvalidParams(format!(
            "Column '{output_col}' already exists. Enable 'Overwrite' to replace it."
        )));
    }
    let expr = FormulaParser::compile(formula, Some(&schema))?;
    let lf = lf.with_columns([expr.alias(output_col)]);
    finish(lf, vec![output_col.to_string()])
}

// ── 2. Conditional derivation / flag variable ───────────────────────────

fn coerce_key(v: &str, dtype: &DataType) -> Expr {
    match dtype {
        dt if dt.is_integer() => {
            if let Ok(i) = v.parse::<i64>() {
                return lit(i);
            }
            lit(v)
        }
        DataType::Float32 | DataType::Float64 => {
            if let Ok(f) = v.parse::<f64>() {
                return lit(f);
            }
            lit(v)
        }
        _ => lit(v),
    }
}

pub fn recode_column(
    lf: LazyFrame,
    column: &str,
    mapping: &[(String, String)],
    default: Option<&str>,
    output_col: Option<&str>,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let output_col = output_col.unwrap_or(column);
    let dtype = schema.get(column).unwrap();

    let base = col(column);
    let mut recode_expr = lit(NULL);
    for (old_val, new_val) in mapping {
        recode_expr = when(base.clone().eq(coerce_key(old_val, dtype)))
            .then(lit(new_val.clone()))
            .otherwise(recode_expr);
    }

    let final_expr = if let Some(default) = default {
        when(recode_expr.clone().is_null()).then(lit(default)).otherwise(recode_expr)
    } else {
        // Keep original where no mapping found; cast to Utf8 for type
        // consistency with the string literals produced by the mapping.
        when(recode_expr.clone().is_null())
            .then(base.cast(DataType::String))
            .otherwise(recode_expr)
    };

    let lf = lf.with_columns([final_expr.alias(output_col)]);
    finish(lf, vec![output_col.to_string()])
}

// ── 3. Discretisation / binning ─────────────────────────────────────────

pub fn discretise_column(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    method: &DiscretiseMethod,
    labels: Option<Vec<String>>,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let output_col = output_col.map(str::to_string).unwrap_or_else(|| format!("{column}_bin"));

    // Bin edges must be concrete f64s (Polars' `cut` takes breakpoints,
    // not expressions) — an intermediate collect is unavoidable here,
    // same as the eager legacy implementation.
    let breakpoints: Vec<f64> = match method {
        DiscretiseMethod::Fixed { bins } => {
            let mut b = bins.clone();
            b.sort_by(|a, b| a.partial_cmp(b).unwrap());
            b
        }
        DiscretiseMethod::EqualWidth { n_bins } => {
            let stats = lf
                .clone()
                .select([col(column).min().alias("__min"), col(column).max().alias("__max")])
                .collect()?;
            let min_val: f64 = stats.column("__min")?.f64()?.get(0).unwrap_or(0.0);
            let max_val: f64 = stats.column("__max")?.f64()?.get(0).unwrap_or(0.0);
            let step = (max_val - min_val) / (*n_bins as f64);
            (1..*n_bins).map(|i| min_val + (i as f64) * step).collect()
        }
        DiscretiseMethod::Quantile { n_bins } => {
            let mut exprs = Vec::new();
            for i in 1..*n_bins {
                exprs.push(
                    col(column)
                        .quantile((i as f64 / *n_bins as f64).into(), QuantileMethod::Linear)
                        .alias(format!("__q{i}")),
                );
            }
            let stats = lf.clone().select(exprs).collect()?;
            (1..*n_bins)
                .map(|i| stats.column(&format!("__q{i}")).unwrap().f64().unwrap().get(0).unwrap_or(0.0))
                .collect()
        }
    };

    let mut cut_expr = col(column).cut(breakpoints, labels, false, false);
    cut_expr = cut_expr.alias(&output_col);
    let lf = lf.with_columns([cut_expr]);
    finish(lf, vec![output_col])
}

// ── 4. Lead and lag variables ───────────────────────────────────────────

pub fn lead_lag(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    periods: i64,
    group_by: Option<&str>,
    fill_value: Option<ScalarValue>,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let output_col = output_col.map(str::to_string).unwrap_or_else(|| {
        if periods < 0 {
            format!("{column}_lead{}", periods.abs())
        } else {
            format!("{column}_lag{periods}")
        }
    });

    let base = match fill_value {
        Some(fv) => col(column).shift_and_fill(lit(periods), fv.to_lit_expr()),
        None => col(column).shift(lit(periods)),
    };
    let expr = match group_by {
        Some(g) => base.over([col(g)]),
        None => base,
    };

    let lf = lf.with_columns([expr.alias(&output_col)]);
    finish(lf, vec![output_col])
}

// ── 5. Fill missing values ──────────────────────────────────────────────

pub fn fill_missing(
    lf: LazyFrame,
    columns: &[String],
    strategy: &FillStrategy,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    for c in columns {
        require_column(&schema, c)?;
    }

    let exprs: Vec<Expr> = columns
        .iter()
        .map(|c| {
            let s = col(c);
            let filled = match strategy {
                FillStrategy::Constant(v) => s.clone().fill_null(v.to_lit_expr()),
                FillStrategy::Mean => s.clone().fill_null(col(c).mean()),
                FillStrategy::Median => s.clone().fill_null(col(c).median()),
                // Polars' `.mode()` order across ties is not documented as
                // stable — flagged (item 3, "needs review") in the port
                // mapping. `.first()` matches the legacy call shape.
                FillStrategy::Mode => s.clone().fill_null(col(c).mode().first()),
                FillStrategy::Forward => s.clone().forward_fill(None),
                FillStrategy::Backward => s.clone().backward_fill(None),
                FillStrategy::Interpolate => s.clone().interpolate(InterpolationMethod::Linear),
            };
            filled.alias(c)
        })
        .collect();

    let lf = lf.with_columns(exprs);
    finish(lf, columns.to_vec())
}

// ── 6. Scalers / normalisation ──────────────────────────────────────────

pub fn apply_scaler(
    lf: LazyFrame,
    columns: &[String],
    method: ScalerMethod,
    output_suffix: Option<&str>,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    for c in columns {
        require_column(&schema, c)?;
    }
    let suffix = output_suffix.unwrap_or("_scaled");

    let mut exprs = Vec::new();
    let mut out_cols = Vec::new();
    for c in columns {
        let out = format!("{c}{suffix}");
        out_cols.push(out.clone());
        let s = col(c);
        let expr = match method {
            ScalerMethod::MinMax => (s.clone() - s.clone().min()) / (s.clone().max() - s.min()),
            ScalerMethod::ZScore => (s.clone() - s.clone().mean()) / s.std(1),
            ScalerMethod::Robust => {
                (s.clone() - s.clone().median())
                    / (s.clone().quantile(0.75.into(), QuantileMethod::Linear)
                        - s.quantile(0.25.into(), QuantileMethod::Linear))
            }
            // Legacy `apply_scaler` documented "boxcox" but never
            // implemented it (dead branch, always raised "Unknown scaler
            // method"). Not ported — there is no legacy behavior to port.
            ScalerMethod::Log => s.log(10.0),
        };
        exprs.push(expr.alias(&out));
    }

    let lf = lf.with_columns(exprs);
    finish(lf, out_cols)
}

// ── 7. Rename columns ────────────────────────────────────────────────────

pub fn rename_columns(
    lf: LazyFrame,
    mapping: &[(String, String)],
) -> Result<super::TransformationResults, TransformError> {
    let existing: Vec<String> = mapping.iter().map(|(old, _)| old.clone()).collect();
    let new_names: Vec<String> = mapping.iter().map(|(_, new)| new.clone()).collect();
    let lf = lf.rename(&existing, &new_names, true);
    finish(lf, new_names)
}

// ── 8. Reorder columns ───────────────────────────────────────────────────

pub fn reorder_columns(
    lf: LazyFrame,
    column_order: &[String],
) -> Result<super::TransformationResults, TransformError> {
    let exprs: Vec<Expr> = column_order.iter().map(|c| col(c)).collect();
    let lf = lf.select(exprs);
    finish(lf, column_order.to_vec())
}

// ── 9. String operations ────────────────────────────────────────────────

pub fn string_op(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    op: &StringOpKind,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let output_col = output_col.unwrap_or(column);
    let s = col(column).cast(DataType::String);

    let expr = match op {
        StringOpKind::Trim => s.str().strip_chars(lit(NULL)),
        StringOpKind::Upper => s.str().to_uppercase(),
        StringOpKind::Lower => s.str().to_lowercase(),
        StringOpKind::Replace { pattern, replacement, regex } => {
            s.str().replace_all(lit(pattern.clone()), lit(replacement.clone()), !*regex)
        }
        StringOpKind::Extract { pattern } => s.str().extract(lit(pattern.clone()), 0),
        StringOpKind::Pad { side, width, fill_char } => match side {
            PadSide::Left => s.str().pad_start(*width, *fill_char),
            PadSide::Right => s.str().pad_end(*width, *fill_char),
        },
    };

    let lf = lf.with_columns([expr.alias(output_col)]);
    finish(lf, vec![output_col.to_string()])
}

// ── 10. Datetime operations ─────────────────────────────────────────────

pub fn datetime_op(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    op: &DatetimeOpKind,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let default_suffix = match op {
        DatetimeOpKind::ExtractYear => "extract_year",
        DatetimeOpKind::ExtractMonth => "extract_month",
        DatetimeOpKind::ExtractDay => "extract_day",
        DatetimeOpKind::ExtractWeekday => "extract_weekday",
        DatetimeOpKind::ExtractHour => "extract_hour",
        DatetimeOpKind::ExtractMinute => "extract_minute",
        DatetimeOpKind::ToUnix => "to_unix",
        DatetimeOpKind::AddDays { .. } => "add_days",
        DatetimeOpKind::Diff { .. } => "diff",
        DatetimeOpKind::Parse { .. } => "parse",
    };
    let output_col = output_col.map(str::to_string).unwrap_or_else(|| format!("{column}_{default_suffix}"));

    let s = col(column);
    let expr = match op {
        DatetimeOpKind::ExtractYear => s.dt().year(),
        DatetimeOpKind::ExtractMonth => s.dt().month(),
        DatetimeOpKind::ExtractDay => s.dt().day(),
        DatetimeOpKind::ExtractWeekday => s.dt().weekday(),
        DatetimeOpKind::ExtractHour => s.dt().hour(),
        DatetimeOpKind::ExtractMinute => s.dt().minute(),
        DatetimeOpKind::ToUnix => s.dt().timestamp(TimeUnit::Milliseconds) / lit(1000),
        DatetimeOpKind::AddDays { days } => {
            s.dt().offset_by(duration(DurationArgs { days: lit(*days), ..Default::default() }))
        }
        DatetimeOpKind::Diff { other_column } => (s - col(other_column)).dt().total_days(),
        DatetimeOpKind::Parse { format } => match format {
            Some(fmt) => s.str().to_datetime(
                None,
                None,
                StrptimeOptions { format: Some(PlSmallStr::from_str(fmt)), strict: true, ..Default::default() },
                lit("raise"),
            ),
            None => s.cast(DataType::Datetime(TimeUnit::Milliseconds, None)),
        },
    };

    let lf = lf.with_columns([expr.alias(&output_col)]);
    finish(lf, vec![output_col])
}

// ── 11. Differencing ────────────────────────────────────────────────────

pub fn differencing(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    periods: i64,
    pct_change: bool,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let output_col = output_col.map(str::to_string).unwrap_or_else(|| format!("{column}_diff{periods}"));

    let expr = if pct_change {
        col(column).pct_change(lit(periods))
    } else {
        col(column).diff(periods, NullBehavior::Ignore)
    };

    let lf = lf.with_columns([expr.alias(&output_col)]);
    finish(lf, vec![output_col])
}

// ── 12. Conditional flag / derived category ─────────────────────────────

fn coerce_scalar_str(val: &str) -> ScalarValue {
    if val.is_empty() {
        return ScalarValue::Null;
    }
    if let Ok(i) = val.parse::<i64>() {
        return ScalarValue::Int(i);
    }
    if let Ok(f) = val.parse::<f64>() {
        return ScalarValue::Float(f);
    }
    ScalarValue::Str(val.to_string())
}

fn build_compare(sub: &crate::types::SubCondition) -> Expr {
    use crate::types::CompareOp::*;
    let col_expr = col(&sub.column);
    let val_expr = sub.value.as_ref().map(|v| v.to_lit_expr());
    let s = col_expr.clone().cast(DataType::String);

    let cmp = match sub.op {
        Eq => col_expr.eq(val_expr.unwrap()),
        Ne => col_expr.neq(val_expr.unwrap()),
        Gt => col_expr.gt(val_expr.unwrap()),
        Lt => col_expr.lt(val_expr.unwrap()),
        Ge => col_expr.gt_eq(val_expr.unwrap()),
        Le => col_expr.lt_eq(val_expr.unwrap()),
        IsNull => col_expr.is_null(),
        IsNotNull => col_expr.is_not_null(),
        Contains => s.str().contains_literal(val_expr.unwrap()),
        StartsWith => s.str().starts_with(val_expr.unwrap()),
        EndsWith => s.str().ends_with(val_expr.unwrap()),
        Matches => s.str().contains(val_expr.unwrap(), false),
    };
    if sub.negate {
        cmp.not()
    } else {
        cmp
    }
}

fn build_case(case: &crate::types::ConditionCase) -> Result<Expr, TransformError> {
    use crate::types::CaseLogic;
    if case.sub_conditions.is_empty() {
        return Err(TransformError::InvalidParams(
            "Each condition case must have at least one sub-condition".into(),
        ));
    }
    let mut iter = case.sub_conditions.iter().map(build_compare);
    let mut combined = iter.next().unwrap();
    for e in iter {
        combined = match case.logic {
            CaseLogic::And => combined.and(e),
            CaseLogic::Or => combined.or(e),
        };
    }
    Ok(combined)
}

pub fn conditional_flag(
    lf: LazyFrame,
    output_col: &str,
    conditions: &[crate::types::ConditionCase],
    default: Option<ScalarValue>,
) -> Result<super::TransformationResults, TransformError> {
    if conditions.is_empty() {
        return Err(TransformError::InvalidParams("At least one condition is required".into()));
    }

    // Built from the last condition backward so the OUTERMOST when/then in
    // the final nested expression is the FIRST condition — this keeps
    // first-match-wins semantics without needing chained `.when()` calls,
    // which change type at each step (When/Then/ChainedWhen/ChainedThen)
    // and can't be folded through a single loop variable.
    let default_expr = default.map(|v| v.to_lit_expr()).unwrap_or(lit(NULL));
    let mut expr = default_expr;
    for case in conditions.iter().rev() {
        expr = when(build_case(case)?).then(case.then.to_lit_expr()).otherwise(expr);
    }
    let expr = expr.alias(output_col);

    let lf = lf.with_columns([expr]);
    finish(lf, vec![output_col.to_string()])
}

// ── 13. Flag values ─────────────────────────────────────────────────────

pub fn flag_values(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    method: &FlagMethod,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let output_col = output_col.map(str::to_string).unwrap_or_else(|| format!("{column}_flag"));
    let s = col(column);

    let expr = match method {
        FlagMethod::OutlierIqr => {
            let stats = lf
                .clone()
                .select([
                    col(column).quantile(0.25.into(), QuantileMethod::Linear).alias("__q1"),
                    col(column).quantile(0.75.into(), QuantileMethod::Linear).alias("__q3"),
                ])
                .collect()?;
            let q1: Option<f64> = stats.column("__q1")?.f64()?.get(0);
            let q3: Option<f64> = stats.column("__q3")?.f64()?.get(0);
            let (Some(q1), Some(q3)) = (q1, q3) else {
                return Err(TransformError::InvalidParams(format!(
                    "Column '{column}' must be numeric with at least one non-null value for IQR outlier detection"
                )));
            };
            let iqr = q3 - q1;
            let (lower, upper) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
            s.clone().lt(lit(lower)).or(s.gt(lit(upper)))
        }
        FlagMethod::OutlierZScore { threshold } => {
            let stats = lf
                .clone()
                .select([col(column).mean().alias("__mean"), col(column).std(1).alias("__std")])
                .collect()?;
            let mean: Option<f64> = stats.column("__mean")?.f64()?.get(0);
            let std: Option<f64> = stats.column("__std")?.f64()?.get(0);
            let (Some(mean), Some(std)) = (mean, std) else {
                return Err(TransformError::InvalidParams(format!(
                    "Column '{column}' must be numeric with at least one non-null value for Z-score outlier detection"
                )));
            };
            if std == 0.0 {
                return Err(TransformError::InvalidParams(format!(
                    "Column '{column}' has zero variance; Z-score outlier detection requires variability"
                )));
            }
            ((s - lit(mean)) / lit(std)).abs().gt(lit(*threshold))
        }
        FlagMethod::TopN { n } => {
            let stats = lf
                .clone()
                .select([col(column)
                    .sort(SortOptions::default().with_order_descending(true))
                    .head(Some(*n))
                    .min()
                    .alias("__t")])
                .collect()?;
            let threshold = stats.column("__t")?.get(0)?;
            s.gt_eq(Expr::Literal(LiteralValue::from(threshold)))
        }
        FlagMethod::BottomN { n } => {
            let stats = lf
                .clone()
                .select([col(column).sort(SortOptions::default()).head(Some(*n)).max().alias("__t")])
                .collect()?;
            let threshold = stats.column("__t")?.get(0)?;
            s.lt_eq(Expr::Literal(LiteralValue::from(threshold)))
        }
        FlagMethod::Duplicates => s.is_duplicated(),
    };

    let lf = lf.with_columns([expr.alias(&output_col)]);
    finish(lf, vec![output_col])
}

// ── 14. Change data type ────────────────────────────────────────────────

fn target_dtype(t: TargetType) -> DataType {
    match t {
        TargetType::Int8 => DataType::Int8,
        TargetType::Int16 => DataType::Int16,
        TargetType::Int32 => DataType::Int32,
        TargetType::Int64 => DataType::Int64,
        TargetType::UInt32 => DataType::UInt32,
        TargetType::UInt64 => DataType::UInt64,
        TargetType::Float32 => DataType::Float32,
        TargetType::Float64 => DataType::Float64,
        TargetType::Utf8 => DataType::String,
        TargetType::Boolean => DataType::Boolean,
        TargetType::Date => DataType::Date,
        TargetType::Datetime => DataType::Datetime(TimeUnit::Milliseconds, None),
    }
}

pub fn change_dtype(
    lf: LazyFrame,
    column: &str,
    target_type: TargetType,
    format: Option<&str>,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let src_dtype = schema.get(column).unwrap().clone();
    let dtype = target_dtype(target_type);

    let src_is_temporal = matches!(src_dtype, DataType::Date | DataType::Datetime(_, _));
    let src_is_boolean = src_dtype == DataType::Boolean;
    let tgt_is_temporal = matches!(dtype, DataType::Date | DataType::Datetime(_, _));
    let tgt_is_boolean = dtype == DataType::Boolean;
    let tgt_is_numeric = dtype.is_numeric();

    if src_is_boolean && tgt_is_temporal {
        return Err(TransformError::InvalidParams(format!(
            "Cannot convert Boolean to {dtype:?}: there is no meaningful date mapping for True/False."
        )));
    }
    if src_is_temporal && tgt_is_boolean {
        return Err(TransformError::InvalidParams(format!(
            "Cannot convert {src_dtype:?} to Boolean: dates have no true/false representation."
        )));
    }
    if src_is_temporal && tgt_is_numeric {
        return Err(TransformError::InvalidParams(format!(
            "Cannot convert {src_dtype:?} to {dtype:?}. Cast to String (Text) first if you need a text representation of the date."
        )));
    }

    let expr = if tgt_is_temporal {
        match format {
            Some(fmt) if dtype == DataType::Date => col(column).str().to_date(StrptimeOptions {
                format: Some(PlSmallStr::from_str(fmt)),
                strict: true,
                ..Default::default()
            }),
            Some(fmt) => col(column).str().to_datetime(
                None,
                None,
                StrptimeOptions { format: Some(PlSmallStr::from_str(fmt)), strict: true, ..Default::default() },
                lit("raise"),
            ),
            None => col(column).strict_cast(dtype),
        }
    } else {
        col(column).strict_cast(dtype)
    };

    let lf = lf.with_columns([expr.alias(column)]);
    finish(lf, vec![column.to_string()]).map_err(|e| match e {
        // strict_cast/to_date/to_datetime raise at collect() time for a
        // value that can't convert — re-wrapped with the same descriptive
        // message the legacy `except Exception as exc` block produced,
        // rather than letting a raw Polars error surface.
        TransformError::Polars(polars_err) => TransformError::InvalidParams(format!(
            "Cannot convert column '{column}' from {src_dtype:?} to {target_type:?}: {polars_err}"
        )),
        other => other,
    })
}

// ── 15. Drop columns ─────────────────────────────────────────────────────

pub fn drop_columns(lf: LazyFrame, columns: &[String]) -> Result<super::TransformationResults, TransformError> {
    let lf = lf.drop(columns.iter().cloned());
    finish(lf, vec![])
}

// ── 16. Copy / clone column ──────────────────────────────────────────────

pub fn copy_column(
    lf: LazyFrame,
    column: &str,
    output_col: &str,
) -> Result<super::TransformationResults, TransformError> {
    let lf = lf.with_columns([col(column).alias(output_col)]);
    finish(lf, vec![output_col.to_string()])
}

// ── 17. Coalesce columns (first non-null) ────────────────────────────────

pub fn coalesce_columns(
    lf: LazyFrame,
    columns: &[String],
    output_col: &str,
) -> Result<super::TransformationResults, TransformError> {
    if columns.len() < 2 {
        return Err(TransformError::InvalidParams("coalesce requires at least 2 columns".into()));
    }
    let exprs: Vec<Expr> = columns.iter().map(|c| col(c)).collect();
    let lf = lf.with_columns([coalesce(&exprs).alias(output_col)]);
    finish(lf, vec![output_col.to_string()])
}

// ── 18. Generate row numbers ─────────────────────────────────────────────

pub fn generate_row_number(
    lf: LazyFrame,
    output_col: Option<&str>,
    start: i64,
) -> Result<super::TransformationResults, TransformError> {
    let output_col = output_col.unwrap_or("row_number");
    let lf = lf.with_row_index("__idx", None).with_columns([(col("__idx").cast(DataType::Int64) + lit(start))
        .alias(output_col)])
        .drop(["__idx"]);
    finish(lf, vec![output_col.to_string()])
}

// ── 19. Generate UUID column ─────────────────────────────────────────────

/// Deterministic when `seed` is given (a Nikti addition — see the
/// Transformation port findings: legacy `generate_uuid` used
/// `uuid.uuid4()` per row with no seed at all, making a Recipe containing
/// it non-reproducible on replay). When `seed` is `None`, behavior matches
/// legacy: OS-random v4 UUIDs, non-deterministic by nature.
pub fn generate_uuid(
    lf: LazyFrame,
    output_col: Option<&str>,
    seed: Option<u64>,
) -> Result<super::TransformationResults, TransformError> {
    let output_col = output_col.unwrap_or("uuid").to_string();
    let df = lf.collect()?;
    let n = df.height();

    let values: Vec<String> = match seed {
        Some(seed) => {
            let mut state = seed;
            (0..n)
                .map(|_| {
                    // splitmix64
                    state = state.wrapping_add(0x9E3779B97F4A7C15);
                    let mut z = state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
                    z ^= z >> 31;
                    let bytes_hi = z.to_be_bytes();
                    let bytes_lo = state.to_be_bytes();
                    let mut bytes = [0u8; 16];
                    bytes[..8].copy_from_slice(&bytes_hi);
                    bytes[8..].copy_from_slice(&bytes_lo);
                    uuid::Uuid::from_bytes(bytes).to_string()
                })
                .collect()
        }
        None => (0..n).map(|_| uuid::Uuid::new_v4().to_string()).collect(),
    };

    let series = Series::new(PlSmallStr::from_str(&output_col), values);
    let df = df.hstack(&[series.into()])?;
    Ok(super::TransformationResults { frame: df, affected_columns: vec![output_col], complement: None })
}

// ── 20. Null standardisation ─────────────────────────────────────────────

const DEFAULT_NULL_VARIANTS: &[&str] = &[
    "", "na", "n/a", "n.a.", "null", "none", "nil", "nan", "missing", "#n/a", "#null!", ".", "-", "--", "unknown",
];

pub fn null_standardise(
    lf: LazyFrame,
    columns: &[String],
    null_variants: Option<Vec<String>>,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    for c in columns {
        require_column(&schema, c)?;
    }
    let variants: Vec<String> = null_variants
        .unwrap_or_else(|| DEFAULT_NULL_VARIANTS.iter().map(|s| s.to_string()).collect())
        .iter()
        .map(|v| v.to_lowercase())
        .collect();

    let exprs: Vec<Expr> = columns
        .iter()
        .map(|c| {
            let check = col(c).cast(DataType::String).str().to_lowercase().is_in(lit(Series::new(
                PlSmallStr::from_str("variants"),
                variants.clone(),
            )));
            when(check).then(lit(NULL)).otherwise(col(c)).alias(c)
        })
        .collect();

    let lf = lf.with_columns(exprs);
    finish(lf, columns.to_vec())
}

// ── 21. Text standardisation ─────────────────────────────────────────────

pub fn text_standardise(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    mapping: &[(String, String)],
    case_sensitive: bool,
) -> Result<super::TransformationResults, TransformError> {
    let schema = schema_of(&lf)?;
    require_column(&schema, column)?;
    let output_col = output_col.unwrap_or(column);

    let (lookup, source_expr): (Vec<(String, String)>, Expr) = if case_sensitive {
        (mapping.to_vec(), col(column).cast(DataType::String))
    } else {
        (
            mapping.iter().map(|(k, v)| (k.to_lowercase(), v.clone())).collect(),
            col(column).cast(DataType::String).str().to_lowercase(),
        )
    };

    let mut expr = lit(NULL).cast(DataType::String);
    for (old_val, new_val) in &lookup {
        expr = when(source_expr.clone().eq(lit(old_val.clone()))).then(lit(new_val.clone())).otherwise(expr);
    }
    let final_expr = when(expr.clone().is_null())
        .then(col(column).cast(DataType::String))
        .otherwise(expr)
        .alias(output_col);

    let lf = lf.with_columns([final_expr]);
    finish(lf, vec![output_col.to_string()])
}

// ── 22. Hash column ───────────────────────────────────────────────────────

pub fn hash_column(
    lf: LazyFrame,
    column: &str,
    output_col: Option<&str>,
    algorithm: HashAlgorithm,
) -> Result<super::TransformationResults, TransformError> {
    let output_col = output_col.map(str::to_string).unwrap_or_else(|| format!("{column}_hash"));
    let df = lf.collect()?;
    let series = df.column(column)?;

    let hashed: Vec<Option<String>> = (0..series.len())
        .map(|i| {
            let av = series.get(i).ok()?;
            if matches!(av, AnyValue::Null) {
                return None;
            }
            // `AnyValue`'s `Display` wraps strings in quotes (it's meant
            // for table pretty-printing) — that would hash `"hello"`
            // instead of `hello`. Use the raw content for strings; other
            // dtypes fall back to Display (numbers format plainly, but
            // note this doesn't guarantee exact parity with Python's
            // `str()` for every dtype, e.g. `True`/`False` vs `true`/
            // `false` — flagged, not silently assumed identical).
            let s = av.get_str().map(|s| s.to_string()).unwrap_or_else(|| av.to_string());
            Some(match algorithm {
                HashAlgorithm::Sha256 => {
                    use sha2::{Digest, Sha256};
                    hex::encode(Sha256::digest(s.as_bytes()))
                }
                HashAlgorithm::Md5 => {
                    use md5::{Digest, Md5};
                    hex::encode(Md5::digest(s.as_bytes()))
                }
                HashAlgorithm::Sha1 => {
                    use sha1::{Digest, Sha1};
                    hex::encode(Sha1::digest(s.as_bytes()))
                }
            })
        })
        .collect();

    let series = Series::new(PlSmallStr::from_str(&output_col), hashed);
    let df = df.hstack(&[series.into()])?;
    Ok(super::TransformationResults { frame: df, affected_columns: vec![output_col], complement: None })
}
