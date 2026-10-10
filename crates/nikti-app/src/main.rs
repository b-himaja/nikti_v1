#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc_commands;

use nikti_core::repositories::dataset_repository::testing::InMemoryDatasetRepository;
use nikti_core::repositories::knowledge_repository::testing::InMemoryKnowledgeRepository;
use nikti_core::repositories::pending_plan_repository::testing::InMemoryPendingPlanRepository;
use nikti_core::repositories::project_repository::testing::InMemoryProjectRepository;
use nikti_core::repositories::report_repository::testing::InMemoryReportRepository;
use nikti_core::repositories::table_repository::testing::InMemoryTableRepository;
use nikti_core::repositories::{DetailTableStore, ParquetContentStore};
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // Table Repository Workspace DI: one shared instance per
        // repository, managed by Tauri and handed to command wrappers as
        // `tauri::State<..>`. Still the `testing::InMemory*` reference
        // implementations (see `ipc_commands` module doc), not the frozen
        // embedded-relational-store — but real, persistent-for-the-life-
        // of-the-app logic, not a mock.
        .manage(InMemoryTableRepository::new())
        .manage(InMemoryReportRepository::new())
        .manage(InMemoryProjectRepository::new())
        .manage(InMemoryDatasetRepository::new())
        .manage(InMemoryKnowledgeRepository::new())
        // Project Workspace UI wiring: pending Transformation Plans have
        // the same in-memory-reference-implementation status as every
        // other repository here — real, stateful for the app's lifetime,
        // not the frozen embedded-relational-store tech decision.
        .manage(InMemoryPendingPlanRepository::new())
        // The content store and detail-table store ARE real filesystem
        // I/O (Parquet, content-addressed) — no test-double stands in for
        // either. Rooted under the OS app-data directory, which is only
        // resolvable once the app is built, hence `.setup()` rather than
        // an eager `.manage()`.
        .setup(|app| {
            let root = app.path().app_data_dir()?.join("content_store");
            app.manage(ParquetContentStore::new(root)?);
            let detail_root = app.path().app_data_dir()?.join("detail_store");
            app.manage(DetailTableStore::new(detail_root)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc_commands::import_table,
            ipc_commands::open_table,
            ipc_commands::inspect_table,
            ipc_commands::validate_table,
            ipc_commands::analyse_native,
            ipc_commands::analyse_python,
            ipc_commands::transform_dataset,
            ipc_commands::transform_dataset_many,
            ipc_commands::apply_recipe,
            ipc_commands::promote_recipe,
            ipc_commands::generate_lineage_report,
            ipc_commands::generate_comparison_report,
            ipc_commands::purge_table,
            ipc_commands::create_project,
            ipc_commands::create_dataset,
            ipc_commands::reimport_table_as_new,
            ipc_commands::search_tables,
            ipc_commands::clear_table_search,
            ipc_commands::refresh_table_repository,
            ipc_commands::open_table_readonly,
            ipc_commands::open_table_information,
            ipc_commands::open_table_dictionary,
            ipc_commands::open_table_event_log,
            ipc_commands::open_table_reports,
            ipc_commands::open_report,
            ipc_commands::load_table_into_project,
            ipc_commands::create_project_from_table,
            ipc_commands::route_table_inspection,
            ipc_commands::route_table_validation,
            ipc_commands::route_table_analysis,
            ipc_commands::archive_table,
            ipc_commands::restore_table,
            ipc_commands::retry_table_availability_check,
            ipc_commands::copy_table_id,
            ipc_commands::table_allowed_actions,
            ipc_commands::open_project,
            ipc_commands::list_projects,
            ipc_commands::load_table_as_dataset,
            ipc_commands::duplicate_dataset,
            ipc_commands::export_dataset_as_table,
            ipc_commands::save_pending_plan,
            ipc_commands::load_pending_plan,
            ipc_commands::run_pending_plan,
            ipc_commands::load_recipe_into_project,
            ipc_commands::run_validation_on_dataset,
            ipc_commands::run_analysis_on_dataset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running nikti-app");
}
