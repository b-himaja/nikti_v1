pub mod analysis;
pub mod content;
pub mod inspection;
pub mod report;
#[cfg(test)]
mod report_analysis_tests;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod report_validation_tests;
pub mod transformation;
pub mod validation;

pub use analysis::{AnalysisEngine, AnalysisError, AnalysisExecutionMetadata, AnalysisResult, AnalysisSummary, AnalysisTarget, NativeAnalysisEngine};
pub use content::{DatasetContent, TableContent};
pub use inspection::{ImportSummary, InspectionEngine, InspectionError, InspectionResults, NativeInspectionEngine};
pub use report::{
    AnalysisReportContent, AnalysisReportSection, GeneratedAnalysisReport, GeneratedAuditReport, GeneratedComparisonReport, GeneratedInspectionReport, GeneratedLineageReport, GeneratedValidationReport,
    NativeReportEngine, ReportEngine, ReportEngineError, ValidationReportContent, ValidationReportSection,
    ANALYSIS_REPORT_SECTION_ORDER_TEMPLATE, VALIDATION_REPORT_SECTION_ORDER,
};
pub use transformation::{NativeTransformationEngine, TransformError, TransformationEngine, TransformationResults, TRANSFORMATION_CATALOGUE_VERSION};
pub use validation::{
    NativeValidationEngine, RuleExecutionResult, RuleOutcome, ValidationEngine, ValidationError, ValidationExecutionMetadata,
    ValidationResults, ValidationSummary, ValidationTarget,
};
