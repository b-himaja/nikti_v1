use std::sync::Arc;

use arrow::array::{Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;

/// Schema for the handshake/heartbeat protocol only. Real Analysis
/// procedure payloads (Arrow batches carrying actual table content) are a
/// later task — this pass only proves the transport is Arrow IPC, per
/// CLAUDE.md Part 5 item 14 (never JSON across this boundary).
pub fn handshake_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("msg_type", DataType::Utf8, false),
        Field::new("python_env_hash", DataType::Utf8, false),
        Field::new("pid", DataType::Int64, false),
        Field::new("timestamp_ms", DataType::Int64, false),
    ]))
}

pub fn make_message(msg_type: &str, python_env_hash: &str) -> RecordBatch {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    RecordBatch::try_new(
        handshake_schema(),
        vec![
            Arc::new(StringArray::from(vec![msg_type])),
            Arc::new(StringArray::from(vec![python_env_hash])),
            Arc::new(Int64Array::from(vec![std::process::id() as i64])),
            Arc::new(Int64Array::from(vec![now_ms])),
        ],
    )
    .expect("handshake_schema() and these arrays must always match")
}

pub fn msg_type(batch: &RecordBatch) -> String {
    batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 0 is msg_type: Utf8 per handshake_schema()")
        .value(0)
        .to_string()
}

pub fn python_env_hash(batch: &RecordBatch) -> String {
    batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 1 is python_env_hash: Utf8 per handshake_schema()")
        .value(0)
        .to_string()
}
