//! Analysis Procedure Catalogue v1.0 — execution logic (Part C). Every
//! procedure is deterministic and never modifies `df`. C.5's closed
//! vocabulary governs every `Finding::statement`/measure name emitted
//! here — `measured`, `observed`, `candidate`, `evidence`,
//! `association`, `boundary candidate`, `dependency candidate`,
//! `attention evidence`, `retention evidence`; never `bad`, `wrong`,
//! `suspicious`, `must drop`, `recommended fix`, `prediction`,
//! `forecast`, bare `anomaly`, or (AP-017 specifically) "feature
//! importance."
//!
//! Every `OutputTableRef` below is backed by a real `DataFrame` returned
//! alongside it (`ProcedureOutcome::output_table_data`) — `row_count`/
//! `columns` on the ref are derived FROM that real frame by `output_table`,
//! never hand-typed, so a claimed-but-uncomputed column is a compile-time-
//! shaped impossibility, not a discipline. A handful of previously-claimed
//! tables had no real computation behind them at all and are no longer
//! claimed here rather than backed with fabricated content — see the
//! per-procedure comments below for exactly which and why (AP-012's
//! regex table, AP-013's date-gap table, AP-016's segment breakdown,
//! AP-018's functional-dependency-exception table).

use std::collections::BTreeMap;

use polars::prelude::*;
use uuid::Uuid;

use crate::types::analysis_procedure_params::{AnalysisProcedureParams as P, BoundaryMethod, CorrelationMethod, DateBucket};
use crate::types::transformation_operation::AggFunc;
use crate::types::AnalysisProcedure;

use super::{AnalysisError, Finding, FindingType, Measure, OutputTableRef, ProcedureResult, VisualOutputRef, VisualOutputType};

#[derive(Default)]
pub struct ProcedureOutcome {
    pub measures: Vec<Measure>,
    pub output_table_refs: Vec<OutputTableRef>,
    /// The real content behind each entry in `output_table_refs`, keyed
    /// by `OutputTableRef::reference` — written to real storage by the
    /// command layer (`project_workspace::run_analysis_on_dataset`), not
    /// by this engine (INV-008: engines compute, the command/repository
    /// layer does storage I/O).
    pub output_table_data: Vec<(String, DataFrame)>,
    pub visual_output_refs: Vec<VisualOutputRef>,
    pub findings: Vec<Finding>,
    pub warnings: Vec<String>,
}

fn measure(name: &str, value: f64, unit: Option<&str>, input_columns: &[&str], method: &str) -> Measure {
    Measure {
        measure_name: name.to_string(),
        value,
        unit: unit.map(|s| s.to_string()),
        input_columns: input_columns.iter().map(|s| s.to_string()).collect(),
        method: method.to_string(),
        missing_policy: "ignore_missing".to_string(),
        denominator: None,
    }
}

/// `row_count`/`columns` are derived from `df` itself, never supplied by
/// the caller — see this module's doc comment.
fn output_table(name: &str, procedure_result_id: Uuid, df: DataFrame) -> (OutputTableRef, DataFrame) {
    let row_count = df.height() as u64;
    let columns: Vec<String> = df.get_column_names().into_iter().map(|s| s.to_string()).collect();
    (OutputTableRef { name: name.to_string(), reference: format!("analysis-output/{procedure_result_id}/{name}.parquet"), row_count, columns }, df)
}

/// Splits a procedure's built-up `(OutputTableRef, DataFrame)` list into
/// the two parallel vectors `ProcedureOutcome` carries — one shared place
/// this happens, not 19 ad hoc ones.
fn split_tables(tables: Vec<(OutputTableRef, DataFrame)>) -> (Vec<OutputTableRef>, Vec<(String, DataFrame)>) {
    let mut refs = Vec::with_capacity(tables.len());
    let mut data = Vec::with_capacity(tables.len());
    for (r, df) in tables {
        data.push((r.reference.clone(), df));
        refs.push(r);
    }
    (refs, data)
}

fn finding(finding_type: FindingType, statement: String, columns: &[&str]) -> Finding {
    Finding {
        finding_id: Uuid::new_v4(),
        finding_type,
        statement,
        evidence_refs: Vec::new(),
        affected_columns: columns.iter().map(|s| s.to_string()).collect(),
        affected_rows_ref: None,
    }
}

fn agg_expr(column: &str, agg: AggFunc) -> Expr {
    let c = col(column);
    match agg {
        AggFunc::Sum => c.sum(),
        AggFunc::Mean => c.mean(),
        AggFunc::Min => c.min(),
        AggFunc::Max => c.max(),
        AggFunc::Count => c.count(),
        AggFunc::First => c.first(),
        AggFunc::Last => c.last(),
        AggFunc::Std => c.std(1),
        AggFunc::Var => c.var(1),
        AggFunc::Median => c.median(),
        AggFunc::NUnique => c.n_unique(),
    }
}

/// Real equal-width histogram binning, shared by AP-009 (numeric values)
/// and AP-015 (text lengths cast to `f64`) — both previously hardcoded
/// `row_count=10` with no binning logic anywhere. `bin_count` bins span
/// `[min, max]`; a degenerate (all-equal-value) input gets a single bin
/// rather than dividing by zero.
fn equal_width_histogram(values: &[f64], bin_count: usize) -> DataFrame {
    if values.is_empty() {
        return df!["bin_start" => Vec::<f64>::new(), "bin_end" => Vec::<f64>::new(), "count" => Vec::<i64>::new()].unwrap();
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let bin_count = bin_count.max(1);
    let width = if max > min { (max - min) / bin_count as f64 } else { 0.0 };

    let mut counts = vec![0i64; bin_count];
    for &v in values {
        let idx = if width > 0.0 { (((v - min) / width) as usize).min(bin_count - 1) } else { 0 };
        counts[idx] += 1;
    }
    let bin_starts: Vec<f64> = (0..bin_count).map(|i| min + i as f64 * width).collect();
    let bin_ends: Vec<f64> = (0..bin_count).map(|i| if width > 0.0 { min + (i + 1) as f64 * width } else { max }).collect();
    df!["bin_start" => bin_starts, "bin_end" => bin_ends, "count" => counts].unwrap()
}

pub fn execute_procedure(
    df: &DataFrame,
    procedure: &AnalysisProcedure,
    procedure_result_id: Uuid,
    prior_results: &[ProcedureResult],
) -> Result<ProcedureOutcome, AnalysisError> {
    match procedure.params() {
        P::ScopeAndVariableRoleSetup => ap001_scope_and_variable_role_setup(df, procedure_result_id),
        P::DerivedMeasureDefinition { measure_name, formula } => ap002_derived_measure_definition(df, measure_name, formula, procedure_result_id),
        P::ColumnPropertyExplorer { columns } => ap003_column_property_explorer(df, columns, procedure_result_id),
        P::GroupedSummary { group_by, measure_column, agg } => ap004_grouped_summary(df, group_by, measure_column, *agg, procedure_result_id),
        P::SegmentSubsetComparison { segment_column, measure_column, agg } => ap005_segment_comparison(df, segment_column, measure_column, *agg, procedure_result_id),
        P::CrossTabulation { row_column, column_column } => ap006_cross_tabulation(df, row_column, column_column, procedure_result_id),
        P::CategoryToNumericComparison { category_column, numeric_column } => ap007_category_to_numeric(df, category_column, numeric_column, procedure_result_id),
        P::NumericRelationshipAssociation { columns, method } => ap008_numeric_relationship(df, columns, *method, procedure_result_id),
        P::DistributionBoundaryExplorer { column, method } => ap009_distribution_boundary(df, column, *method, procedure_result_id),
        P::MissingnessStructureExplorer { columns } => ap010_missingness_structure(df, columns, procedure_result_id),
        P::RepetitionDuplicateStructureExplorer { key_columns } => ap011_repetition_duplicate(df, key_columns, procedure_result_id),
        P::PatternFormatStructureExplorer { column, regex_list } => ap012_pattern_format(df, column, regex_list.as_deref(), procedure_result_id),
        P::DateTimeTrendExplorer { date_column, bucket, duration_start_column, duration_end_column } => {
            ap013_date_time_trend(df, date_column, *bucket, duration_start_column.as_deref(), duration_end_column.as_deref(), procedure_result_id)
        }
        P::ParetoConcentrationExplorer { category_column, measure_column } => ap014_pareto_concentration(df, category_column, measure_column, procedure_result_id),
        P::TextMeasurementExplorer { column, keywords } => ap015_text_measurement(df, column, keywords, procedure_result_id),
        P::TargetLens { target_column, segment_columns } => ap016_target_lens(df, target_column, segment_columns.as_deref(), procedure_result_id),
        P::AssociationStrengthRanking { target_column, candidate_columns } => ap017_association_strength_ranking(df, target_column, candidate_columns, procedure_result_id),
        P::DependencyCandidateExplorer { columns } => ap018_dependency_candidate_explorer(df, columns, procedure_result_id),
        P::FeatureAndRowAttentionScreen => ap019_feature_and_row_attention_screen(df, prior_results, procedure_result_id),
    }
}

// AP-001 — Scope & Variable Role Setup (mandatory, B.7).
fn ap001_scope_and_variable_role_setup(df: &DataFrame, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let row_count = df.height() as u64;
    let column_count = df.width() as u64;
    let (output_table_refs, output_table_data) = split_tables(vec![output_table(
        "scope_summary_table",
        procedure_result_id,
        df![ "row_count" => [row_count as i64], "column_count" => [column_count as i64] ]?,
    )]);
    Ok(ProcedureOutcome {
        measures: vec![
            measure("scope_row_count", row_count as f64, None, &[], "measured"),
            measure("scope_column_count", column_count as f64, None, &[], "measured"),
        ],
        output_table_refs,
        output_table_data,
        visual_output_refs: Vec::new(),
        findings: vec![finding(FindingType::MeasuredObservation, format!("Scope resolved to {row_count} rows and {column_count} columns."), &[])],
        warnings: Vec::new(),
    })
}

// AP-002 — Derived Measure Definition. Deterministic expression
// evaluation only — never written back to the Dataset/Table.
fn ap002_derived_measure_definition(df: &DataFrame, measure_name: &str, formula: &str, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let schema = df.schema();
    let compiled = crate::engines::transformation::formula::FormulaParser::compile(formula, Some(&schema)).map_err(|e| AnalysisError::Request(e.to_string()))?;
    let out = df.clone().lazy().select([compiled.cast(DataType::Float64).mean().alias(measure_name)]).collect()?;
    let mean = out.column(measure_name)?.f64()?.get(0).unwrap_or(0.0);
    let (output_table_refs, output_table_data) = split_tables(vec![output_table("derived_measure_summary_table", procedure_result_id, out)]);
    Ok(ProcedureOutcome {
        measures: vec![measure(&format!("{measure_name}_mean"), mean, None, &[], "expression")],
        output_table_refs,
        output_table_data,
        visual_output_refs: Vec::new(),
        findings: vec![finding(FindingType::MeasuredObservation, format!("Derived measure '{measure_name}' has an observed mean of {mean:.4}."), &[])],
        warnings: Vec::new(),
    })
}

// AP-003 — Column Property Explorer.
fn ap003_column_property_explorer(df: &DataFrame, columns: &[String], procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let cols: Vec<String> = if columns.is_empty() { df.get_column_names().into_iter().map(|s| s.to_string()).collect() } else { columns.to_vec() };
    let mut measures = Vec::new();
    let mut names = Vec::new();
    let mut distinct_counts = Vec::new();
    let mut null_counts = Vec::new();
    for c in &cols {
        let s = df.column(c)?;
        let distinct = s.as_materialized_series().n_unique()? as i64;
        let nulls = s.null_count() as i64;
        measures.push(measure(&format!("{c}_distinct_count"), distinct as f64, None, &[c], "measured"));
        measures.push(measure(&format!("{c}_null_count"), nulls as f64, None, &[c], "measured"));
        names.push(c.clone());
        distinct_counts.push(distinct);
        null_counts.push(nulls);
    }
    let (output_table_refs, output_table_data) = split_tables(vec![output_table(
        "column_property_table",
        procedure_result_id,
        df!["column" => names, "distinct_count" => distinct_counts, "null_count" => null_counts]?,
    )]);
    Ok(ProcedureOutcome { measures, output_table_refs, output_table_data, visual_output_refs: Vec::new(), findings: Vec::new(), warnings: Vec::new() })
}

// AP-004 — Grouped Summary.
fn ap004_grouped_summary(df: &DataFrame, group_by: &[String], measure_column: &str, agg: AggFunc, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let out = df
        .clone()
        .lazy()
        .group_by(group_by.iter().map(|c| col(c)).collect::<Vec<_>>())
        .agg([agg_expr(measure_column, agg).alias("value")])
        .sort(["value"], SortMultipleOptions::default().with_order_descending(true))
        .collect()?;
    let group_count = out.height() as u64;

    // Real rank over the already-sorted `value` column — same `.rank()`
    // mechanism AP-008 already uses for Spearman, not a new algorithm.
    let ranked = out
        .clone()
        .lazy()
        .with_columns([col("value").rank(RankOptions { method: RankMethod::Ordinal, descending: true }, None).cast(DataType::Int64).alias("rank")])
        .collect()?;
    let rank_table = ranked.select(group_by.iter().cloned().chain(["rank".to_string(), "value".to_string()]))?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("group_summary_table", procedure_result_id, out),
        output_table("group_rank_table", procedure_result_id, rank_table),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![measure("group_count", group_count as f64, None, &[measure_column], "group_by")],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::BarChart, reference: format!("analysis-output/{procedure_result_id}/group_summary_chart") }],
        findings: Vec::new(),
        warnings: Vec::new(),
    })
}

// AP-005 — Segment/Subset Comparison.
fn ap005_segment_comparison(df: &DataFrame, segment_column: &str, measure_column: &str, agg: AggFunc, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let out = df.clone().lazy().group_by([col(segment_column)]).agg([agg_expr(measure_column, agg).alias("value")]).collect()?;
    let segment_count = out.height() as u64;
    let values: Vec<f64> = out.column("value")?.cast(&DataType::Float64)?.f64()?.into_iter().flatten().collect();
    let max = values.iter().cloned().fold(f64::MIN, f64::max);
    let min = values.iter().cloned().fold(f64::MAX, f64::min);

    // Real per-segment difference from the overall (whole-dataset)
    // aggregate — the overall aggregate is the same `agg` applied to the
    // unsegmented data, not a new statistic.
    let overall_df = df.clone().lazy().select([agg_expr(measure_column, agg).cast(DataType::Float64).alias("__overall")]).collect()?;
    let overall = overall_df.column("__overall")?.f64()?.get(0).unwrap_or(0.0);
    let difference_table = out
        .clone()
        .lazy()
        .with_columns([(col("value").cast(DataType::Float64) - lit(overall)).alias("difference_from_overall")])
        .select([col(segment_column), col("difference_from_overall")])
        .collect()?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("segment_comparison_table", procedure_result_id, out),
        output_table("difference_table", procedure_result_id, difference_table),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![measure("segment_count", segment_count as f64, None, &[segment_column], "group_by"), measure("segment_value_spread", max - min, None, &[measure_column], "max_minus_min")],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::GroupedBarChart, reference: format!("analysis-output/{procedure_result_id}/segment_chart") }],
        findings: Vec::new(),
        warnings: Vec::new(),
    })
}

fn chi_squared_and_cramers_v(counts: &BTreeMap<(String, String), u64>, row_totals: &BTreeMap<String, u64>, col_totals: &BTreeMap<String, u64>, n: u64) -> (f64, f64) {
    let n = n as f64;
    let mut chi2 = 0.0;
    for (row, row_total) in row_totals {
        for (col_name, col_total) in col_totals {
            let observed = *counts.get(&(row.clone(), col_name.clone())).unwrap_or(&0) as f64;
            let expected = (*row_total as f64) * (*col_total as f64) / n;
            if expected > 0.0 {
                chi2 += (observed - expected).powi(2) / expected;
            }
        }
    }
    let min_dim = ((row_totals.len().max(1) - 1).max(1)).min((col_totals.len().max(1) - 1).max(1)) as f64;
    let cramers_v = if n > 0.0 && min_dim > 0.0 { (chi2 / (n * min_dim)).sqrt() } else { 0.0 };
    (chi2, cramers_v)
}

// AP-006 — Cross-Tabulation.
fn ap006_cross_tabulation(df: &DataFrame, row_column: &str, column_column: &str, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let sub = df.select([row_column, column_column])?.lazy().with_columns([col(row_column).cast(DataType::String), col(column_column).cast(DataType::String)]).collect()?;
    let row_ca = sub.column(row_column)?.str()?;
    let col_ca = sub.column(column_column)?.str()?;
    let mut counts: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut row_totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut col_totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut n = 0u64;
    for (r, c) in row_ca.into_iter().zip(col_ca.into_iter()) {
        if let (Some(r), Some(c)) = (r, c) {
            *counts.entry((r.to_string(), c.to_string())).or_insert(0) += 1;
            *row_totals.entry(r.to_string()).or_insert(0) += 1;
            *col_totals.entry(c.to_string()).or_insert(0) += 1;
            n += 1;
        }
    }
    let (chi2, cramers_v) = chi_squared_and_cramers_v(&counts, &row_totals, &col_totals, n);

    let mut rows = Vec::new();
    let mut cols = Vec::new();
    let mut count_vals = Vec::new();
    let mut pct_vals = Vec::new();
    for ((r, c), count) in &counts {
        rows.push(r.clone());
        cols.push(c.clone());
        count_vals.push(*count as i64);
        pct_vals.push(if n > 0 { *count as f64 / n as f64 * 100.0 } else { 0.0 });
    }
    let count_table = df![row_column => rows.clone(), column_column => cols.clone(), "count" => count_vals]?;
    let percentage_table = df![row_column => rows, column_column => cols, "percentage" => pct_vals]?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("crosstab_count_table", procedure_result_id, count_table),
        output_table("crosstab_percentage_table", procedure_result_id, percentage_table),
        output_table("association_summary_table", procedure_result_id, df!["chi_squared" => [chi2], "cramers_v" => [cramers_v]]?),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![
            measure("chi_squared", chi2, None, &[row_column, column_column], "chi_squared"),
            measure("cramers_v", cramers_v, None, &[row_column, column_column], "cramers_v"),
        ],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::Heatmap, reference: format!("analysis-output/{procedure_result_id}/crosstab_heatmap") }],
        findings: vec![finding(
            FindingType::AssociationCandidate,
            format!("Observed association candidate between '{row_column}' and '{column_column}' (Cramér's V = {cramers_v:.4})."),
            &[row_column, column_column],
        )],
        warnings: Vec::new(),
    })
}

// AP-007 — Category-to-Numeric Comparison (grouped summaries + eta squared).
fn ap007_category_to_numeric(df: &DataFrame, category_column: &str, numeric_column: &str, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let grand_mean_df = df.clone().lazy().select([col(numeric_column).cast(DataType::Float64).mean().alias("__gm")]).collect()?;
    let grand_mean = grand_mean_df.column("__gm")?.f64()?.get(0).unwrap_or(0.0);

    let groups = df
        .clone()
        .lazy()
        .group_by([col(category_column)])
        .agg([col(numeric_column).cast(DataType::Float64).mean().alias("mean"), col(numeric_column).count().alias("count")])
        .collect()?;
    let means = groups.column("mean")?.f64()?;
    let ns = groups.column("count")?.u32()?;

    let full = df.clone().lazy().select([col(numeric_column).cast(DataType::Float64)]).collect()?;
    let values: Vec<f64> = full.column(numeric_column)?.f64()?.into_iter().flatten().collect();
    let ss_total: f64 = values.iter().map(|v| (v - grand_mean).powi(2)).sum();
    let mut ss_between = 0.0;
    for i in 0..groups.height() {
        if let (Some(m), Some(n)) = (means.get(i), ns.get(i)) {
            ss_between += n as f64 * (m - grand_mean).powi(2);
        }
    }
    let eta_squared = if ss_total > 0.0 { ss_between / ss_total } else { 0.0 };

    // Real per-row (category, numeric_value) pairs — the raw data a
    // box-plot renders from, not an aggregate summary (that's
    // `category_numeric_summary_table`, a different, already-real table).
    let distribution_table = df.select([category_column, numeric_column])?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("category_numeric_summary_table", procedure_result_id, groups),
        output_table("category_distribution_table", procedure_result_id, distribution_table),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![measure("eta_squared", eta_squared, None, &[category_column, numeric_column], "eta_squared")],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::BoxPlot, reference: format!("analysis-output/{procedure_result_id}/category_numeric_boxplot") }],
        findings: vec![finding(
            FindingType::AssociationCandidate,
            format!("Observed association candidate between '{category_column}' and '{numeric_column}' (eta squared = {eta_squared:.4})."),
            &[category_column, numeric_column],
        )],
        warnings: Vec::new(),
    })
}

// AP-008 — Numeric Relationship & Association ("pairwise numeric
// coefficients" — C.2, never "correlation matrix").
fn ap008_numeric_relationship(df: &DataFrame, columns: &[String], method: CorrelationMethod, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let lf = df.clone().lazy();
    let mut measures = Vec::new();
    let mut a_names = Vec::new();
    let mut b_names = Vec::new();
    let mut coefficients = Vec::new();
    for i in 0..columns.len() {
        for j in (i + 1)..columns.len() {
            let (a, b) = (&columns[i], &columns[j]);
            let (expr_a, expr_b) = match method {
                CorrelationMethod::Pearson => (col(a), col(b)),
                CorrelationMethod::Spearman => (col(a).rank(RankOptions::default(), None), col(b).rank(RankOptions::default(), None)),
            };
            let out = lf.clone().select([pearson_corr(expr_a, expr_b, 1).alias("__corr")]).collect()?;
            let coeff = out.column("__corr")?.f64()?.get(0).unwrap_or(f64::NAN);
            measures.push(measure(&format!("{a}__{b}__coefficient"), coeff, None, &[a, b], if method == CorrelationMethod::Pearson { "pearson" } else { "spearman" }));
            a_names.push(a.clone());
            b_names.push(b.clone());
            coefficients.push(coeff);
        }
    }
    let pair_count = a_names.len() as u64;
    let pairwise_table = df!["column_a" => a_names.clone(), "column_b" => b_names.clone(), "coefficient" => coefficients.clone()]?;

    // Real "top" pairs — sorted by absolute coefficient strength,
    // descending, capped at 10 (unambiguous reading of "top numeric
    // pairs": strongest absolute association first).
    let mut ranked: Vec<(String, String, f64)> = a_names.into_iter().zip(b_names).zip(coefficients).map(|((a, b), c)| (a, b, c)).collect();
    ranked.sort_by(|x, y| y.2.abs().partial_cmp(&x.2.abs()).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(10);
    let (top_a, top_b, top_c): (Vec<String>, Vec<String>, Vec<f64>) = ranked.into_iter().fold((Vec::new(), Vec::new(), Vec::new()), |mut acc, (a, b, c)| {
        acc.0.push(a);
        acc.1.push(b);
        acc.2.push(c);
        acc
    });
    let top_table = df!["column_a" => top_a, "column_b" => top_b, "coefficient" => top_c]?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("pairwise_numeric_coefficient_table", procedure_result_id, pairwise_table),
        output_table("top_numeric_pairs_table", procedure_result_id, top_table),
    ]);
    let _ = pair_count;
    Ok(ProcedureOutcome {
        measures,
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::ScatterPlot, reference: format!("analysis-output/{procedure_result_id}/pairwise_scatter") }],
        findings: Vec::new(),
        warnings: Vec::new(),
    })
}

// AP-009 — Distribution & Boundary Explorer.
fn ap009_distribution_boundary(df: &DataFrame, column: &str, method: BoundaryMethod, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let stats = df
        .clone()
        .lazy()
        .select([
            col(column).cast(DataType::Float64).quantile(0.25.into(), QuantileMethod::Linear).alias("q1"),
            col(column).cast(DataType::Float64).quantile(0.5.into(), QuantileMethod::Linear).alias("q2"),
            col(column).cast(DataType::Float64).quantile(0.75.into(), QuantileMethod::Linear).alias("q3"),
            col(column).cast(DataType::Float64).mean().alias("mean"),
            col(column).cast(DataType::Float64).std(1).alias("std"),
        ])
        .collect()?;
    let q1 = stats.column("q1")?.f64()?.get(0).unwrap_or(0.0);
    let q3 = stats.column("q3")?.f64()?.get(0).unwrap_or(0.0);
    let mean = stats.column("mean")?.f64()?.get(0).unwrap_or(0.0);
    let std = stats.column("std")?.f64()?.get(0).unwrap_or(0.0);
    let iqr = q3 - q1;

    let (lower, upper) = match method {
        BoundaryMethod::Iqr => (q1 - 1.5 * iqr, q3 + 1.5 * iqr),
        BoundaryMethod::ZScore => (mean - 3.0 * std, mean + 3.0 * std),
        BoundaryMethod::Percentile => {
            let p = df.clone().lazy().select([col(column).cast(DataType::Float64).quantile(0.01.into(), QuantileMethod::Linear).alias("__p1"), col(column).cast(DataType::Float64).quantile(0.99.into(), QuantileMethod::Linear).alias("__p99")]).collect()?;
            (p.column("__p1")?.f64()?.get(0).unwrap_or(0.0), p.column("__p99")?.f64()?.get(0).unwrap_or(0.0))
        }
    };

    let beyond_df = df.clone().lazy().filter(col(column).cast(DataType::Float64).lt(lit(lower)).or(col(column).cast(DataType::Float64).gt(lit(upper)))).collect()?;
    let beyond_count = beyond_df.height() as u64;

    let mut findings = Vec::new();
    if beyond_count > 0 {
        findings.push(finding(
            FindingType::BoundaryCandidate,
            format!("{beyond_count} value(s) in '{column}' fall outside the boundary candidate range [{lower:.4}, {upper:.4}]."),
            &[column],
        ));
    }

    // Real equal-width histogram over the column's real values.
    let all_values: Vec<f64> = df.clone().lazy().select([col(column).cast(DataType::Float64)]).collect()?.column(column)?.f64()?.into_iter().flatten().collect();
    let histogram = equal_width_histogram(&all_values, 10);

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("distribution_summary_table", procedure_result_id, stats),
        output_table("histogram_table", procedure_result_id, histogram),
        output_table("boundary_candidate_table", procedure_result_id, beyond_df.select([column])?),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![
            measure("iqr", iqr, None, &[column], "iqr"),
            measure("boundary_lower", lower, None, &[column], "boundary"),
            measure("boundary_upper", upper, None, &[column], "boundary"),
            measure("boundary_candidate_count", beyond_count as f64, None, &[column], "boundary"),
        ],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::Histogram, reference: format!("analysis-output/{procedure_result_id}/distribution_histogram") }],
        findings,
        warnings: Vec::new(),
    })
}

// AP-010 — Missingness Structure Explorer (phi coefficient between two
// columns' missingness indicators).
fn ap010_missingness_structure(df: &DataFrame, columns: &[String], procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let cols: Vec<String> = if columns.is_empty() { df.get_column_names().into_iter().map(|s| s.to_string()).collect() } else { columns.to_vec() };
    let mut measures = Vec::new();
    let mut names = Vec::new();
    let mut ratios = Vec::new();
    for c in &cols {
        let ratio = df.column(c)?.null_count() as f64 / df.height().max(1) as f64;
        measures.push(measure(&format!("{c}_missing_ratio"), ratio, None, &[c], "measured"));
        names.push(c.clone());
        ratios.push(ratio);
    }
    let summary_table = df!["column" => names, "missing_ratio" => ratios]?;

    // Real per-row missingness pattern across the bound columns — a
    // "1"/"0" flag per column, comma-joined, one row per df row.
    let missing_flags: Vec<Series> = cols.iter().map(|c| df.column(c).unwrap().is_null().into_series()).collect();
    let mut patterns = Vec::with_capacity(df.height());
    for row in 0..df.height() {
        let flags: Vec<&str> = missing_flags.iter().map(|s| if s.get(row).map(|v| v == AnyValue::Boolean(true)).unwrap_or(false) { "1" } else { "0" }).collect();
        patterns.push(flags.join(","));
    }
    let pattern_table = df!["pattern" => patterns]?;

    let mut pair_names_a = Vec::new();
    let mut pair_names_b = Vec::new();
    let mut phis = Vec::new();
    for i in 0..cols.len() {
        for j in (i + 1)..cols.len() {
            let (a, b) = (&cols[i], &cols[j]);
            let out = df
                .clone()
                .lazy()
                .select([col(a).is_null().cast(DataType::Float64).alias("__a"), col(b).is_null().cast(DataType::Float64).alias("__b")])
                .select([pearson_corr(col("__a"), col("__b"), 1).alias("__phi")])
                .collect()?;
            let phi = out.column("__phi")?.f64()?.get(0).unwrap_or(0.0);
            measures.push(measure(&format!("{a}__{b}__missingness_phi"), phi, None, &[a, b], "phi_coefficient"));
            pair_names_a.push(a.clone());
            pair_names_b.push(b.clone());
            phis.push(phi);
        }
    }
    let cooccurrence_table = df!["column_a" => pair_names_a, "column_b" => pair_names_b, "phi" => phis]?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("missingness_summary_table", procedure_result_id, summary_table),
        output_table("missingness_pattern_table", procedure_result_id, pattern_table),
        output_table("missingness_cooccurrence_table", procedure_result_id, cooccurrence_table),
    ]);
    Ok(ProcedureOutcome {
        measures,
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::MatrixView, reference: format!("analysis-output/{procedure_result_id}/missingness_matrix") }],
        findings: Vec::new(),
        warnings: Vec::new(),
    })
}

// AP-011 — Repetition/Duplicate Structure Explorer.
fn ap011_repetition_duplicate(df: &DataFrame, key_columns: &[String], procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let scope = if key_columns.is_empty() { df.clone() } else { df.select(key_columns.iter().cloned())? };
    let dup_mask = scope.is_duplicated()?;
    let dup_count = dup_mask.into_iter().filter(|v| v == &Some(true)).count() as u64;
    let group_keys: Vec<Expr> =
        if key_columns.is_empty() { df.get_column_names().into_iter().map(|s| col(s.as_str())).collect() } else { key_columns.iter().map(|c| col(c.as_str())).collect() };
    // Real per-key repeat counts — the same group-by already used to
    // derive `group_count`, kept as a table instead of only its height.
    let group_table = df.clone().lazy().group_by(group_keys).agg([len().alias("repeat_count")]).sort(["repeat_count"], SortMultipleOptions::default().with_order_descending(true)).collect()?;
    let group_count = group_table.height() as u64;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("duplicate_summary_table", procedure_result_id, df!["exact_duplicate_row_count" => [dup_count as i64], "distinct_key_group_count" => [group_count as i64]]?),
        output_table("duplicate_group_table", procedure_result_id, group_table),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![
            measure("exact_duplicate_row_count", dup_count as f64, None, &[], "measured"),
            measure("distinct_key_group_count", group_count as f64, None, key_columns.iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice(), "measured"),
        ],
        output_table_refs,
        output_table_data,
        visual_output_refs: Vec::new(),
        findings: if dup_count > 0 { vec![finding(FindingType::MeasuredObservation, format!("{dup_count} exact duplicate row(s) observed."), &[])] } else { Vec::new() },
        warnings: Vec::new(),
    })
}

fn format_shape(v: &str) -> String {
    v.chars().map(|c| if c.is_ascii_digit() { '9' } else if c.is_ascii_uppercase() { 'A' } else if c.is_ascii_lowercase() { 'a' } else { c }).collect()
}

// AP-012 — Pattern/Format Structure Explorer.
fn ap012_pattern_format(df: &DataFrame, column: &str, regex_list: Option<&[String]>, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let ca = df.column(column)?.cast(&DataType::String)?;
    let ca = ca.str()?;
    let mut shapes: BTreeMap<String, u64> = BTreeMap::new();
    let mut total = 0u64;
    for v in ca.into_iter().flatten() {
        *shapes.entry(format_shape(v)).or_insert(0) += 1;
        total += 1;
    }
    let distinct = shapes.len() as u64;
    let (pattern_names, pattern_counts): (Vec<String>, Vec<i64>) = shapes.into_iter().map(|(k, v)| (k, v as i64)).unzip();
    let pattern_table = df!["pattern" => pattern_names, "count" => pattern_counts]?;

    let mut tables = vec![output_table("pattern_frequency_table", procedure_result_id, pattern_table)];
    let mut measures = vec![measure("distinct_format_count", distinct as f64, None, &[column], "measured"), measure("sample_count", total as f64, None, &[column], "measured")];
    let mut warnings = Vec::new();

    // Real per-regex match/non-match counts against `column`'s non-null
    // values, using the catalogue's `regex list` parameter — the
    // parameter this sub-table had no way to receive before this task
    // (`PatternFormatStructureExplorer::regex_list`). `None` (or an empty
    // list) means this ruleset version didn't configure any regex, in
    // which case the table is not executable for this run — no fallback,
    // no fabricated content.
    match regex_list.filter(|list| !list.is_empty()) {
        Some(patterns) => {
            let non_null_count = (df.height() - df.column(column)?.null_count()) as i64;
            let mut regex_col = Vec::with_capacity(patterns.len());
            let mut match_col = Vec::with_capacity(patterns.len());
            let mut non_match_col = Vec::with_capacity(patterns.len());
            for pattern in patterns {
                let matched = df
                    .clone()
                    .lazy()
                    .filter(col(column).is_not_null())
                    .select([col(column).cast(DataType::String).str().contains(lit(pattern.clone()), true).cast(DataType::Int64).sum().alias("match_count")])
                    .collect()?;
                let match_count = matched.column("match_count")?.i64()?.get(0).unwrap_or(0);
                let non_match_count = non_null_count - match_count;
                measures.push(measure(&format!("{pattern}__regex_match_count"), match_count as f64, None, &[column], "regex_contains"));
                measures.push(measure(&format!("{pattern}__regex_non_match_count"), non_match_count as f64, None, &[column], "regex_contains"));
                regex_col.push(pattern.clone());
                match_col.push(match_count);
                non_match_col.push(non_match_count);
            }
            let regex_measure_table = df!["regex" => regex_col, "match_count" => match_col, "non_match_count" => non_match_col]?;
            tables.push(output_table("regex_measure_table", procedure_result_id, regex_measure_table));
        }
        None => {
            warnings.push("regex_measure_table not executed: regex_list was not supplied for this ruleset version".to_string());
        }
    }

    let (output_table_refs, output_table_data) = split_tables(tables);
    Ok(ProcedureOutcome {
        measures,
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::BarChart, reference: format!("analysis-output/{procedure_result_id}/pattern_frequency_chart") }],
        findings: Vec::new(),
        warnings,
    })
}

// AP-013 — Date/Time & Trend Explorer. No forecasting.
fn ap013_date_time_trend(
    df: &DataFrame,
    date_column: &str,
    bucket: DateBucket,
    duration_start_column: Option<&str>,
    duration_end_column: Option<&str>,
    procedure_result_id: Uuid,
) -> Result<ProcedureOutcome, AnalysisError> {
    let truncate_by = match bucket {
        DateBucket::Day => "1d",
        DateBucket::Week => "1w",
        DateBucket::Month => "1mo",
    };
    let bucketed = df
        .clone()
        .lazy()
        .select([col(date_column).cast(DataType::Datetime(TimeUnit::Milliseconds, None)).dt().truncate(lit(truncate_by)).alias("bucket")])
        .group_by([col("bucket")])
        .agg([len().alias("count")])
        .sort(["bucket"], SortMultipleOptions::default())
        .collect()?;
    let bucket_count = bucketed.height() as u64;
    let stats = df
        .clone()
        .lazy()
        .select([
            col(date_column).cast(DataType::Datetime(TimeUnit::Milliseconds, None)).min().alias("min"),
            col(date_column).cast(DataType::Datetime(TimeUnit::Milliseconds, None)).max().alias("max"),
        ])
        .collect()?;
    let min = stats.column("min")?.cast(&DataType::String)?.str()?.get(0).unwrap_or("").to_string();
    let max = stats.column("max")?.cast(&DataType::String)?.str()?.get(0).unwrap_or("").to_string();

    // `date_gap_table` (missing-bucket detection) is deliberately not
    // claimed here (Part B): unlike duration statistics, "which buckets
    // are missing from the series" needs its own gap-detection algorithm
    // this task's Part C didn't scope — removed, not faked. It is a
    // structurally different table from `duration_summary_table` below
    // (different inputs, different statistic) and neither one's absence
    // or presence should ever be read as evidence about the other.
    let mut tables = vec![output_table("date_summary_table", procedure_result_id, stats), output_table("time_bucket_table", procedure_result_id, bucketed)];
    let mut warnings = Vec::new();

    // Real duration statistics: min/max/mean/median of the PER-ROW
    // difference `duration_end_column - duration_start_column`, in days
    // — the catalogue's `duration start/end` parameter pair, and the same
    // "duration" definition AP-002 uses (e.g. `delay_days = payment_date
    // - invoice_date`). This is NOT the gap between consecutive
    // observations in `date_column` (that is `date_gap_table`'s concept,
    // deliberately absent above) — an earlier fix corrected the copy-
    // paste bug that made this table identical to `date_summary_table`
    // but computed that between-row gap statistic instead, which is a
    // different table's definition wearing this one's name. No fallback
    // to that computation is used here.
    match (duration_start_column, duration_end_column) {
        (Some(start_col), Some(end_col)) => {
            let durations_df = df
                .clone()
                .lazy()
                .select([((col(end_col).cast(DataType::Datetime(TimeUnit::Milliseconds, None)).cast(DataType::Int64)
                    - col(start_col).cast(DataType::Datetime(TimeUnit::Milliseconds, None)).cast(DataType::Int64))
                .cast(DataType::Float64)
                    / lit(86_400_000.0))
                .alias("__duration_days")])
                .drop_nulls(None)
                .collect()?;
            let duration_summary = if durations_df.height() > 0 {
                durations_df
                    .lazy()
                    .select([
                        col("__duration_days").min().alias("min"),
                        col("__duration_days").max().alias("max"),
                        col("__duration_days").mean().alias("mean"),
                        col("__duration_days").median().alias("median"),
                    ])
                    .collect()?
            } else {
                df!["min" => [f64::NAN], "max" => [f64::NAN], "mean" => [f64::NAN], "median" => [f64::NAN]]?
            };
            tables.push(output_table("duration_summary_table", procedure_result_id, duration_summary));
        }
        _ => {
            // Rule 9's discipline applied to a missing optional parameter,
            // not just a sidecar failure: record the gap with a reason,
            // produce zero partial/fabricated evidence for this specific
            // table, and let the rest of the procedure (date_summary_table,
            // time_bucket_table) complete normally — this is a per-table
            // gap, not a whole-procedure `NotExecutable`.
            warnings.push("duration_summary_table not executed: duration_start_column/duration_end_column were not supplied for this ruleset version".to_string());
        }
    }

    let (output_table_refs, output_table_data) = split_tables(tables);
    Ok(ProcedureOutcome {
        measures: vec![measure("time_bucket_count", bucket_count as f64, None, &[date_column], "date_truncate")],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::LineChart, reference: format!("analysis-output/{procedure_result_id}/time_bucket_chart") }],
        findings: vec![finding(FindingType::MeasuredObservation, format!("'{date_column}' observed range: {min} to {max}."), &[date_column])],
        warnings,
    })
}

// AP-014 — Pareto/Concentration Explorer.
fn ap014_pareto_concentration(df: &DataFrame, category_column: &str, measure_column: &str, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let grouped = df.clone().lazy().group_by([col(category_column)]).agg([col(measure_column).cast(DataType::Float64).sum().alias("sum")]).sort(["sum"], SortMultipleOptions::default().with_order_descending(true)).collect()?;
    let values: Vec<f64> = grouped.column("sum")?.f64()?.into_iter().flatten().collect();
    let total: f64 = values.iter().sum();
    let hhi: f64 = if total > 0.0 { values.iter().map(|v| (v / total * 100.0).powi(2)).sum() } else { 0.0 };
    let mut cumulative = 0.0;
    let mut categories_for_80pct = 0u64;
    let mut cumulative_shares = Vec::with_capacity(values.len());
    for (i, v) in values.iter().enumerate() {
        cumulative += v;
        let share = if total > 0.0 { cumulative / total } else { 0.0 };
        cumulative_shares.push(share);
        if categories_for_80pct == 0 && total > 0.0 && share >= 0.8 {
            categories_for_80pct = (i + 1) as u64;
        }
    }
    if categories_for_80pct == 0 {
        categories_for_80pct = values.len() as u64;
    }
    let cumulative_share_series = Series::new("cumulative_share".into(), cumulative_shares);
    let pareto_table = grouped.hstack(&[cumulative_share_series.into()])?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("pareto_table", procedure_result_id, pareto_table),
        output_table("concentration_summary_table", procedure_result_id, df!["hhi" => [hhi], "categories_for_80pct_share" => [categories_for_80pct as i64]]?),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![
            measure("hhi", hhi, None, &[category_column, measure_column], "hhi"),
            measure("categories_for_80pct_share", categories_for_80pct as f64, None, &[category_column], "cumulative_share"),
        ],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::ParetoChart, reference: format!("analysis-output/{procedure_result_id}/pareto_chart") }],
        findings: Vec::new(),
        warnings: Vec::new(),
    })
}

// AP-015 — Text Measurement Explorer. No sentiment/topic modeling.
fn ap015_text_measurement(df: &DataFrame, column: &str, keywords: &[String], procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let ca = df.column(column)?.cast(&DataType::String)?;
    let ca = ca.str()?;
    let lengths: Vec<u64> = ca.into_iter().flatten().map(|v| v.chars().count() as u64).collect();
    let mean_length = if lengths.is_empty() { 0.0 } else { lengths.iter().sum::<u64>() as f64 / lengths.len() as f64 };
    let length_values: Vec<i64> = lengths.iter().map(|&l| l as i64).collect();
    let text_metric_table = df![column => df.column(column)?.as_materialized_series().clone(), "length" => length_values]?;

    let mut keyword_counts: BTreeMap<String, u64> = BTreeMap::new();
    for v in ca.into_iter().flatten() {
        let lowered = v.to_lowercase();
        for k in keywords {
            if lowered.contains(&k.to_lowercase()) {
                *keyword_counts.entry(k.clone()).or_insert(0) += 1;
            }
        }
    }
    let (kw_names, kw_counts): (Vec<String>, Vec<i64>) = keyword_counts.into_iter().map(|(k, v)| (k, v as i64)).unzip();
    let keyword_table = df!["keyword" => kw_names, "count" => kw_counts]?;

    // Real equal-width histogram over text lengths, same shared binning
    // logic AP-009 uses for numeric values.
    let length_f64: Vec<f64> = lengths.iter().map(|&l| l as f64).collect();
    let length_distribution = equal_width_histogram(&length_f64, 10);

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("text_metric_table", procedure_result_id, text_metric_table),
        output_table("length_distribution_table", procedure_result_id, length_distribution),
        output_table("keyword_frequency_table", procedure_result_id, keyword_table),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![measure("mean_length", mean_length, Some("characters"), &[column], "measured")],
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::Histogram, reference: format!("analysis-output/{procedure_result_id}/length_distribution_chart") }],
        findings: Vec::new(),
        warnings: Vec::new(),
    })
}

// AP-016 — Target Lens.
fn ap016_target_lens(df: &DataFrame, target_column: &str, segment_columns: Option<&[String]>, procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let dtype = df.column(target_column)?.dtype().clone();
    let mut measures = Vec::new();
    let profile_table;
    if dtype.is_numeric() {
        let stats = df.clone().lazy().select([col(target_column).cast(DataType::Float64).mean().alias("mean"), col(target_column).cast(DataType::Float64).std(1).alias("std")]).collect()?;
        measures.push(measure("target_mean", stats.column("mean")?.f64()?.get(0).unwrap_or(0.0), None, &[target_column], "measured"));
        measures.push(measure("target_std", stats.column("std")?.f64()?.get(0).unwrap_or(0.0), None, &[target_column], "measured"));
        profile_table = stats;
    } else {
        let n = df.column(target_column)?.as_materialized_series().n_unique()? as f64;
        measures.push(measure("target_distinct_count", n, None, &[target_column], "measured"));
        profile_table = df!["distinct_count" => [n]]?;
    }
    let coverage = 1.0 - df.column(target_column)?.null_count() as f64 / df.height().max(1) as f64;
    measures.push(measure("target_coverage_ratio", coverage, None, &[target_column], "measured"));

    let mut tables = vec![output_table("target_profile_table", procedure_result_id, profile_table), output_table("target_coverage_table", procedure_result_id, df!["coverage_ratio" => [coverage]]?)];
    let mut warnings = Vec::new();

    // Real per-segment breakdown of the SAME target-profile measures
    // computed above (mean/std for a numeric target, distinct_count for
    // a categorical one, plus per-segment coverage) — this is deliberately
    // NOT `ap005_segment_comparison`'s machinery: AP-005 aggregates a
    // caller-chosen `measure_column` with a caller-chosen `AggFunc`,
    // while AP-016 profiles `target_column` itself the same way the
    // whole-dataset profile above does. `None` (or an empty list) means
    // this ruleset version didn't configure `segment_columns`, in which
    // case the table is not executable for this run — no fallback to
    // AP-005's shape, no fabricated content.
    match segment_columns.filter(|cols| !cols.is_empty()) {
        Some(segments) => {
            let group_keys: Vec<Expr> = segments.iter().map(|c| col(c.as_str())).collect();
            let mut agg_exprs = vec![len().alias("segment_row_count")];
            if dtype.is_numeric() {
                agg_exprs.push(col(target_column).cast(DataType::Float64).mean().alias("target_mean"));
                agg_exprs.push(col(target_column).cast(DataType::Float64).std(1).alias("target_std"));
            } else {
                agg_exprs.push(col(target_column).n_unique().cast(DataType::Float64).alias("target_distinct_count"));
            }
            agg_exprs.push((lit(1.0) - col(target_column).is_null().cast(DataType::Float64).mean()).alias("target_coverage_ratio"));
            let segment_table = df.clone().lazy().group_by(group_keys).agg(agg_exprs).sort(segments.to_vec(), SortMultipleOptions::default()).collect()?;
            measures.push(measure("target_by_segment_count", segment_table.height() as f64, None, segments.iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice(), "group_by"));
            tables.push(output_table("target_by_segment_table", procedure_result_id, segment_table));
        }
        None => {
            warnings.push("target_by_segment_table not executed: segment_columns was not supplied for this ruleset version".to_string());
        }
    }

    let (output_table_refs, output_table_data) = split_tables(tables);
    Ok(ProcedureOutcome { measures, output_table_refs, output_table_data, visual_output_refs: Vec::new(), findings: Vec::new(), warnings })
}

// AP-017 — Association Strength Ranking. Never "feature importance."
fn ap017_association_strength_ranking(df: &DataFrame, target_column: &str, candidate_columns: &[String], procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let target_numeric = df.column(target_column)?.dtype().is_numeric();
    let mut measures = Vec::new();
    let mut names = Vec::new();
    let mut strengths = Vec::new();
    let mut methods = Vec::new();
    for c in candidate_columns {
        let candidate_numeric = df.column(c)?.dtype().is_numeric();
        let (strength, method_used) = if target_numeric && candidate_numeric {
            let out = df.clone().lazy().select([pearson_corr(col(target_column).cast(DataType::Float64), col(c).cast(DataType::Float64), 1).alias("__c")]).collect()?;
            (out.column("__c")?.f64()?.get(0).unwrap_or(0.0).abs(), "pearson")
        } else {
            (0.0, "unavailable")
        };
        measures.push(measure(&format!("{c}_association_strength"), strength, None, &[target_column, c], "bivariate_association"));
        names.push(c.clone());
        strengths.push(strength);
        methods.push(method_used.to_string());
    }
    let ranking_table = df!["candidate_column" => names.clone(), "association_strength" => strengths]?;
    let detail_table = df!["candidate_column" => names, "method" => methods]?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("association_ranking_table", procedure_result_id, ranking_table),
        output_table("association_detail_table", procedure_result_id, detail_table),
    ]);
    Ok(ProcedureOutcome {
        measures,
        output_table_refs,
        output_table_data,
        visual_output_refs: vec![VisualOutputRef { visual_type: VisualOutputType::BarChart, reference: format!("analysis-output/{procedure_result_id}/association_ranking_chart") }],
        findings: candidate_columns
            .iter()
            .map(|c| finding(FindingType::AssociationCandidate, format!("Observed a bivariate association candidate between '{target_column}' and '{c}'."), &[target_column, c]))
            .collect(),
        warnings: Vec::new(),
    })
}

// AP-018 — Dependency Candidate Explorer. Single-object only.
fn ap018_dependency_candidate_explorer(df: &DataFrame, columns: &[String], procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let cols: Vec<String> = if columns.is_empty() { df.get_column_names().into_iter().map(|s| s.to_string()).collect() } else { columns.to_vec() };
    let mut measures = Vec::new();
    let mut columns_with_100pct_uniqueness = Vec::new();
    let mut names = Vec::new();
    let mut ratios = Vec::new();
    for c in &cols {
        let n_unique = df.column(c)?.as_materialized_series().n_unique()? as u64;
        let is_unique = n_unique as usize == df.height() && df.height() > 0;
        if is_unique {
            columns_with_100pct_uniqueness.push(c.clone());
        }
        let ratio = n_unique as f64 / df.height().max(1) as f64;
        measures.push(measure(&format!("{c}_distinct_ratio"), ratio, None, &[c], "measured"));
        names.push(c.clone());
        ratios.push(ratio);
    }
    let candidate_table = df!["column" => names, "distinct_ratio" => ratios]?;
    let uniqueness_table = df!["column" => columns_with_100pct_uniqueness.clone()]?;

    // `fd_exception_table` is deliberately not claimed here (Part B):
    // functional-dependency-exception detection (for every determinant/
    // dependent column pair, count rows violating the mapping) is a real
    // algorithm this task's Part C didn't scope — only single-column
    // uniqueness is computed today. Removed, not faked.
    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("dependency_candidate_table", procedure_result_id, candidate_table),
        output_table("composite_uniqueness_table", procedure_result_id, uniqueness_table),
    ]);
    Ok(ProcedureOutcome {
        measures,
        output_table_refs,
        output_table_data,
        visual_output_refs: Vec::new(),
        // C.2: "columns with 100% uniqueness" — never "candidate key" in
        // user-facing text (the internal dependency_type value is a
        // separate, non-user-facing concept this codebase doesn't need
        // to model further for V1).
        findings: if columns_with_100pct_uniqueness.is_empty() {
            Vec::new()
        } else {
            vec![finding(
                FindingType::DependencyCandidate,
                format!("Columns with 100% uniqueness observed: {}.", columns_with_100pct_uniqueness.join(", ")),
                &columns_with_100pct_uniqueness.iter().map(|s| &s[..]).collect::<Vec<_>>(),
            )]
        },
        warnings: Vec::new(),
    })
}

// AP-019 — Feature & Row Attention Screen (Amendment 6: "retention
// review"/"row attention review," never "drop review"; never an
// automatic retain/drop/delete/fix recommendation — evidence only).
fn ap019_feature_and_row_attention_screen(df: &DataFrame, prior_results: &[ProcedureResult], procedure_result_id: Uuid) -> Result<ProcedureOutcome, AnalysisError> {
    let boundary_or_dependency_findings: Vec<&Finding> = prior_results
        .iter()
        .flat_map(|r| r.findings.iter())
        .filter(|f| matches!(f.finding_type, FindingType::BoundaryCandidate | FindingType::DependencyCandidate))
        .collect();

    let mut findings = Vec::new();
    let mut retention_columns = Vec::new();
    let mut retention_evidence = Vec::new();
    for f in &boundary_or_dependency_findings {
        let statement = format!("Retention review evidence: {}", f.statement);
        findings.push(finding(FindingType::RetentionEvidence, statement.clone(), &f.affected_columns.iter().map(|s| s.as_str()).collect::<Vec<_>>()));
        retention_columns.push(f.affected_columns.first().cloned().unwrap_or_default());
        retention_evidence.push(statement);
    }

    let high_missing_columns: Vec<String> = df
        .get_column_names()
        .into_iter()
        .filter(|c| (df.column(c).unwrap().null_count() as f64 / df.height().max(1) as f64) >= 0.5)
        .map(|s| s.to_string())
        .collect();
    let mut row_attention_evidence = Vec::new();
    for c in &high_missing_columns {
        let statement = format!("Row attention evidence: column '{c}' has high observed missingness — a candidate for review before validation or transformation.");
        findings.push(finding(FindingType::RowAttentionEvidence, statement.clone(), &[c]));
        row_attention_evidence.push(statement);
    }

    let retention_table = df!["column" => retention_columns, "evidence" => retention_evidence]?;
    let row_attention_table = df!["column" => high_missing_columns.clone(), "evidence" => row_attention_evidence]?;

    let (output_table_refs, output_table_data) = split_tables(vec![
        output_table("feature_retention_evidence_table", procedure_result_id, retention_table),
        output_table("row_attention_table", procedure_result_id, row_attention_table),
    ]);
    Ok(ProcedureOutcome {
        measures: vec![measure("retention_review_candidate_count", boundary_or_dependency_findings.len() as f64, None, &[], "aggregated_evidence")],
        output_table_refs,
        output_table_data,
        visual_output_refs: Vec::new(),
        findings,
        warnings: Vec::new(),
    })
}
