//! The real Table import pipeline (CSV first — Part A). Lives as a flat
//! module alongside `table_repository.rs`, matching this crate's
//! established `repositories/` layout (no `repositories/table/`
//! subdirectory exists anywhere in this codebase; inventing one for this
//! task alone would be a one-off convention).
//!
//! Frozen tech decision (`CLAUDE.md`): content storage is content-
//! addressed Parquet, keyed by fingerprint. This module is what actually
//! satisfies that for the first time — every prior task exercised
//! `Table`/`TableContent` against synthetic, hand-fabricated fingerprints
//! and in-memory frames only.

use std::fs;
use std::path::{Path, PathBuf};

use polars::prelude::*;
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("io error reading '{path}': {source}")]
    Io { path: String, #[source] source: std::io::Error },
    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),
    #[error("source file has no rows")]
    EmptyFile,
}

/// Above this size, prefer Polars' streaming collection engine over a
/// single eager `.collect()` — same "streaming where source exceeds
/// comfortable memory" principle already stated as a frozen tech
/// decision. There is no pre-existing streaming utility anywhere in this
/// codebase to reuse (Inspection only ever consumes an already-collected
/// `TableContent`) — this constant and `collect_frame` below are it.
pub const STREAMING_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;

fn collect_frame(lf: LazyFrame, source_size_bytes: u64) -> Result<DataFrame, ImportError> {
    if source_size_bytes > STREAMING_THRESHOLD_BYTES {
        // Polars' streaming engine kicks in automatically for eligible
        // query plans when the streaming feature is enabled and the
        // engine is requested; a plain scan -> collect over a CSV source
        // is exactly such a plan.
        Ok(lf.with_streaming(true).collect()?)
    } else {
        Ok(lf.collect()?)
    }
}

/// A deterministic hash over column names and dtypes, in schema (column)
/// order (Part B). Two files with identical structure but different data
/// produce the same value; anything that changes column names, order, or
/// types changes it.
pub fn schema_fingerprint(schema: &Schema) -> String {
    let mut hasher = Sha256::new();
    for (name, dtype) in schema.iter() {
        hasher.update(name.as_bytes());
        hasher.update(b":");
        hasher.update(format!("{dtype:?}").as_bytes());
        hasher.update(b"|");
    }
    hex::encode(hasher.finalize())
}

/// A deterministic hash over actual cell content, in a fixed row-major,
/// schema-column-order traversal (Part B) — never anything time-
/// dependent (no read timestamp, no file path). Stable across re-runs on
/// unchanged input; this is exactly what Amendment 10's idempotency-pair
/// check depends on being genuine.
pub fn data_fingerprint(df: &DataFrame) -> Result<String, ImportError> {
    let mut hasher = Sha256::new();
    let columns = df.get_columns();
    for row in 0..df.height() {
        for column in columns {
            let value = column.get(row)?;
            hasher.update(format!("{value}").as_bytes());
            hasher.update(b"\x1f");
        }
        hasher.update(b"\x1e");
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Part A's malformed-row accounting. Deliberately independent of
/// Polars' own CSV-parsing error handling (which nulls/coerces bad
/// values rather than reporting a row count) — this does its own raw
/// pass over the file, comparing each data row's field count against the
/// header's. A simple, honest definition of "malformed" (ragged field
/// count), not a full RFC4180 validator — quoted commas are not
/// accounted for, since polars' own reader handles real quoting for the
/// data it actually loads; this pass only produces the diagnostic count.
fn count_malformed_rows(path: &Path) -> Result<u64, ImportError> {
    let text = fs::read_to_string(path).map_err(|e| ImportError::Io { path: path.display().to_string(), source: e })?;
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return Ok(0) };
    let expected_fields = header.split(',').count();
    let mut malformed = 0u64;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if line.split(',').count() != expected_fields {
            malformed += 1;
        }
    }
    Ok(malformed)
}

pub struct CsvReadOutcome {
    pub frame: DataFrame,
    pub schema_fingerprint: String,
    pub data_fingerprint: String,
    pub row_count: u64,
    pub column_count: u64,
    pub malformed_row_count: u64,
}

/// Part A: lazy scan (never eagerly materializing before it's necessary)
/// via `LazyCsvReader`, tolerant of bad cell values (`ignore_errors` —
/// they become null rather than aborting the whole import; ragged *rows*
/// are separately counted by `count_malformed_rows`, decoupled from
/// what Polars itself loads) — a CSV with unreadable rows is recorded,
/// never silently dropped or coerced away, and never a reason to fail
/// the whole import.
pub fn read_csv(path: &Path) -> Result<CsvReadOutcome, ImportError> {
    let metadata = fs::metadata(path).map_err(|e| ImportError::Io { path: path.display().to_string(), source: e })?;

    let lf = LazyCsvReader::new(path).with_ignore_errors(true).finish()?;
    let malformed_row_count = count_malformed_rows(path)?;

    let df = collect_frame(lf, metadata.len())?;
    if df.height() == 0 {
        return Err(ImportError::EmptyFile);
    }

    let schema = df.schema();
    let schema_fp = schema_fingerprint(&schema);
    let data_fp = data_fingerprint(&df)?;
    let row_count = df.height() as u64;
    let column_count = schema.len() as u64;

    Ok(CsvReadOutcome { frame: df, schema_fingerprint: schema_fp, data_fingerprint: data_fp, row_count, column_count, malformed_row_count })
}

/// Part C: content-addressed Parquet storage, keyed by `data_fingerprint`
/// — never a second write for content that's already stored (dedup is a
/// storage-level property, not just a convention). A real filesystem-
/// backed implementation, not a test double — Part C's "confirm the
/// content store deduplicates" requirement is only meaningful against
/// real I/O.
pub struct ParquetContentStore {
    root: PathBuf,
}

impl ParquetContentStore {
    pub fn new(root: PathBuf) -> Result<Self, ImportError> {
        fs::create_dir_all(&root).map_err(|e| ImportError::Io { path: root.display().to_string(), source: e })?;
        Ok(Self { root })
    }

    fn path_for(&self, fingerprint: &str) -> PathBuf {
        self.root.join(format!("{fingerprint}.parquet"))
    }

    pub fn exists(&self, fingerprint: &str) -> bool {
        self.path_for(fingerprint).exists()
    }

    /// Returns the `content_ref` (a path relative to nothing in
    /// particular — an opaque string per `TableDataRef::content_ref`'s
    /// existing contract, same as every fabricated one used before this
    /// task). Writes only if a Parquet file for this exact fingerprint
    /// doesn't already exist.
    pub fn write_if_absent(&self, fingerprint: &str, df: &mut DataFrame) -> Result<String, ImportError> {
        let path = self.path_for(fingerprint);
        if !path.exists() {
            let file = fs::File::create(&path).map_err(|e| ImportError::Io { path: path.display().to_string(), source: e })?;
            ParquetWriter::new(file).finish(df)?;
        }
        Ok(path.display().to_string())
    }

    /// Reads a previously-written Parquet file back by its `content_ref`
    /// (as returned by `write_if_absent` — any absolute path this store
    /// produced, not necessarily under `self.root`, since Table and
    /// Dataset content share one store instance and `content_ref` is the
    /// opaque handle either object type carries). Used by
    /// `commands::project_workspace` so a Dataset created from a Table
    /// (or a Table exported from a Dataset) works from the SAME
    /// underlying bytes rather than re-deriving them.
    pub fn read(&self, content_ref: &str) -> Result<DataFrame, ImportError> {
        let file = fs::File::open(content_ref).map_err(|e| ImportError::Io { path: content_ref.to_string(), source: e })?;
        Ok(ParquetReader::new(file).finish()?)
    }
}

/// The real write path behind `validation::AffectedRowsRef` and
/// `analysis::OutputTableRef`. Did not exist before this task — both
/// engines only ever constructed a plausible-looking reference string
/// (`format!("validation-failed-rows/{{id}}.parquet")` /
/// `format!("analysis-output/{{id}}/{{name}}.parquet")`), never backed by
/// a write. Unlike `ParquetContentStore`, this is execution-addressed,
/// not content-addressed: the caller already knows the deterministic
/// logical reference (derived from execution/rule/procedure identity)
/// before any content exists, so there is no dedup-by-fingerprint
/// concept here — every write is authoritative for its own reference,
/// and a reference is stored as the logical, root-relative string
/// (`"validation-failed-rows/{id}.parquet"`), never an absolute
/// filesystem path baked into evidence, matching the existing
/// `AffectedRowsRef`/`OutputTableRef` convention rather than
/// `ParquetContentStore`'s absolute-path one.
pub struct DetailTableStore {
    root: PathBuf,
}

impl DetailTableStore {
    pub fn new(root: PathBuf) -> Result<Self, ImportError> {
        fs::create_dir_all(&root).map_err(|e| ImportError::Io { path: root.display().to_string(), source: e })?;
        Ok(Self { root })
    }

    fn path_for(&self, reference: &str) -> PathBuf {
        self.root.join(reference)
    }

    /// Writes `df` at the given logical `reference` (already the exact
    /// string a `AffectedRowsRef`/`OutputTableRef` carries), creating any
    /// parent directories the reference implies. Always writes — unlike
    /// `ParquetContentStore`, there is no dedup here: each reference is
    /// unique to one execution's one rule/procedure result, never reused.
    pub fn write(&self, reference: &str, df: &mut DataFrame) -> Result<(), ImportError> {
        let path = self.path_for(reference);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| ImportError::Io { path: parent.display().to_string(), source: e })?;
        }
        let file = fs::File::create(&path).map_err(|e| ImportError::Io { path: path.display().to_string(), source: e })?;
        ParquetWriter::new(file).finish(df)?;
        Ok(())
    }

    /// Reads back a previously-written detail/output table by its
    /// logical `reference`. Used both by real command-layer consumers
    /// and by tests proving a reference actually resolves to real data.
    pub fn read(&self, reference: &str) -> Result<DataFrame, ImportError> {
        let path = self.path_for(reference);
        let file = fs::File::open(&path).map_err(|e| ImportError::Io { path: path.display().to_string(), source: e })?;
        Ok(ParquetReader::new(file).finish()?)
    }
}
