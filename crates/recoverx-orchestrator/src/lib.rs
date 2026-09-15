//! Recovery Orchestrator
//!
//! Coordinates the complete recovery pipeline:
//! SOURCE → VALIDATION → PARTITION DETECTION → FILESYSTEM DETECTION →
//! METADATA ANALYSIS → DELETED FILE ANALYSIS → FILE CARVING →
//! RESULT NORMALIZATION → DUPLICATE DETECTION → CONFIDENCE SCORING →
//! INDEX RESULTS → USER REVIEW → RECOVERY

pub mod database;
pub mod orchestrator;
pub mod session;
pub mod task;
pub mod worker;

pub use database::SessionDatabase;
pub use orchestrator::RecoveryOrchestrator;
pub use session::ScanSession;
pub use task::{PipelineTask, TaskType};
