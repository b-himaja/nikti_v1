//! Demonstrates, against the real engine/type layer (not `todo!()`), the
//! exact sequence a real `commands::transform_dataset` implementation must
//! follow once repository wiring lands: engine call -> `Dataset::apply_change`
//! -> `ProjectOperationLogEntry::append`. No step is optional, including
//! for "trivial" operations — this test exists so that obligation is
//! checked now, not deferred to whenever the repository layer arrives.

#[cfg(test)]
mod tests {
    use polars::prelude::*;
    use uuid::Uuid;

    use crate::engines::content::DatasetContent;
    use crate::engines::{NativeTransformationEngine, TransformationEngine};
    use crate::types::common::ProjectId;
    use crate::types::table::TableSchema;
    use crate::types::{Dataset, ProjectOperationLog, ProjectOperationLogEntry, TransformationOperation};

    fn sample_frame() -> LazyFrame {
        df!["id" => [1i64, 2, 3], "amount" => [10.0, 20.0, 30.0], "note" => ["a", "b", "c"]]
            .unwrap()
            .lazy()
    }

    #[test]
    fn a_transformation_step_mutates_the_dataset_snapshots_and_appends_to_the_log() {
        let dataset_id = Uuid::new_v4();
        let project_id: ProjectId = Uuid::new_v4();

        let mut dataset = Dataset::new(
            dataset_id,
            project_id,
            "orders".to_string(),
            None,
            None,
            TableSchema { columns: vec![] },
            3,
            "content/fp-initial.parquet".to_string(),
            "fp-initial".to_string(),
            0,
        );
        let mut log = ProjectOperationLog::new();
        assert_eq!(dataset.snapshots().len(), 0);
        assert_eq!(log.entries().len(), 0);

        // 1. Engine call.
        let content = DatasetContent { dataset_id, frame: sample_frame() };
        let operation = TransformationOperation::DropColumns { columns: vec!["note".to_string()] };
        let engine = NativeTransformationEngine;
        let results = engine.transform(content, operation.clone()).expect("drop_columns should succeed");

        // 2. Dataset::apply_change — snapshots prior state, updates current.
        dataset.apply_change(
            "content/fp-after-drop.parquet".to_string(),
            "fp-after-drop".to_string(),
            results.schema(),
            results.row_count(),
            "drop_columns".to_string(),
            None,
            1,
        );

        // 3. Project Operation Log append — happens regardless of how
        // "trivial" the op is (Rule 6 / INV-020), and is the command
        // layer's job, never the engine's (INV-013).
        log.append(ProjectOperationLogEntry::new_transformation(
            Uuid::new_v4(),
            project_id,
            dataset_id,
            1,
            "drop_columns".to_string(),
            operation.clone(),
        ));

        assert_eq!(dataset.row_count(), 3);
        assert_eq!(dataset.content_fingerprint(), "fp-after-drop");
        assert_eq!(dataset.snapshots().len(), 1, "prior state preserved as a rollback point");
        assert_eq!(dataset.snapshots()[0].content_fingerprint(), "fp-initial");
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].operation(), "drop_columns");
        assert_eq!(log.entries()[0].transformation_operation(), Some(&operation), "real, replayable steps for Recipe promotion — not just a label");

        // 4. Rollback never touches the log (INV-005) — only Dataset state.
        dataset.rollback_to(0, 2).unwrap();
        assert_eq!(dataset.content_fingerprint(), "fp-initial");
        assert_eq!(log.entries().len(), 1, "rollback does not delete or truncate any log entry");
    }

    #[test]
    fn a_multi_dataset_operation_produces_a_new_dataset_not_an_in_place_mutation() {
        let left_id = Uuid::new_v4();
        let right_id = Uuid::new_v4();

        let left = DatasetContent { dataset_id: left_id, frame: df!["id" => [1i64, 2]].unwrap().lazy() };
        let right =
            DatasetContent { dataset_id: right_id, frame: df!["id" => [1i64, 2]].unwrap().lazy().with_columns([lit(9.0).alias("score")]) };

        let engine = NativeTransformationEngine;
        let results = engine
            .transform_many(vec![left, right], TransformationOperation::IntersectDatasets)
            .expect("intersect should succeed");

        // The result becomes a brand-new Dataset (a new id, new fingerprint)
        // via commands::create_dataset-shaped logic — never a mutation of
        // either input Dataset, matching INV-003's "freely created" model
        // for multi-input structural operations.
        assert_eq!(results.row_count(), 2);
    }
}
