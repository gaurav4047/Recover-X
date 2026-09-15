//! Recovery Orchestrator.
//!
//! Coordinates the complete recovery pipeline:
//! SOURCE → VALIDATION → PARTITION DETECTION → FILESYSTEM DETECTION →
//! METADATA ANALYSIS → DELETED FILE ANALYSIS → FILE CARVING →
//! RESULT NORMALIZATION → DUPLICATE DETECTION → CONFIDENCE SCORING →
//! INDEX RESULTS

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use recoverx_core::{
    error::{RecoverXError, Result},
    events::{
        EventBus, FilesystemDetectedEvent, PartitionDetectedEvent, ScanCompletedEvent,
        ScanFailedEvent, ScanPausedEvent, ScanProgressEvent, ScanResumedEvent, ScanStartedEvent,
        TaskStateChangedEvent,
    },
    identity::SourceIdentity,
    types::{ScanConfiguration, SessionId, SessionStatus, TaskStatus},
};
use recoverx_engines::{
    filesystem::{FilesystemDetector, Fat32Analyzer},
    partition::PartitionDetector,
    carving::FileCarver,
    RecoveredFile,
};
use recoverx_storage::{DiskImageSource, PhysicalDeviceSource};

use crate::{
    database::SessionDatabase,
    session::ScanSession,
    task::{build_pipeline, PipelineTask, TaskType},
};

/// The primary coordinator for recovery operations.
pub struct RecoveryOrchestrator {
    db: Arc<Mutex<SessionDatabase>>,
    pub event_bus: EventBus,
    #[allow(dead_code)]
    data_dir: PathBuf,
}

impl RecoveryOrchestrator {
    /// Create a new orchestrator backed by `data_dir`.
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("sessions.db");
        let db = SessionDatabase::open(&db_path)?;

        Ok(RecoveryOrchestrator {
            db: Arc::new(Mutex::new(db)),
            event_bus: EventBus::new(),
            data_dir,
        })
    }

    /// Subscribe to events emitted by the orchestrator.
    pub fn subscribe(
        &self,
    ) -> tokio::sync::broadcast::Receiver<recoverx_core::events::RecoverXEvent> {
        self.event_bus.subscribe()
    }

    /// Create a new scan session and persist it.
    pub fn create_session(
        &self,
        source_identity: SourceIdentity,
        configuration: ScanConfiguration,
    ) -> Result<ScanSession> {
        let session = ScanSession::new(source_identity, configuration);
        let db = self.db.lock().expect("db lock poisoned");
        db.insert_session(&session)?;
        tracing::info!(
            session_id = %session.id,
            source = %session.source_identity.path,
            mode = %session.configuration.mode,
            "Scan session created"
        );
        Ok(session)
    }

    pub fn list_sessions(&self) -> Result<Vec<ScanSession>> {
        let db = self.db.lock().expect("db lock poisoned");
        db.list_sessions()
    }

    pub fn load_session(&self, id: &SessionId) -> Result<ScanSession> {
        let db = self.db.lock().expect("db lock poisoned");
        db.load_session(id)
    }

    pub fn delete_session(&self, id: &SessionId) -> Result<()> {
        let db = self.db.lock().expect("db lock poisoned");
        db.delete_session(id)
    }

    /// Cancel a session — marks it as cancelled and persists to DB.
    pub fn cancel_session(&self, id: &SessionId) -> Result<()> {
        let db = self.db.lock().expect("db lock poisoned");
        let mut session = db.load_session(id)?;
        session.mark_cancelled();
        db.update_session(&session)?;
        tracing::info!(session_id = %id, "Scan cancelled by user");
        Ok(())
    }

    /// Update session in DB (public for command handlers).
    pub fn update_session(&self, session: &ScanSession) -> Result<()> {
        let db = self.db.lock().expect("db lock poisoned");
        db.update_session(session)
    }

    /// List recovered files for a session.
    pub fn list_recovered_files(&self, session_id: &str) -> Result<Vec<RecoveredFile>> {
        let db = self.db.lock().expect("db lock poisoned");
        db.list_recovered_files(session_id)
    }

    /// List partitions for a session.
    pub fn list_partitions(&self, session_id: &str) -> Result<Vec<recoverx_engines::DetectedPartition>> {
        let db = self.db.lock().expect("db lock poisoned");
        db.list_partitions(session_id)
    }

    /// Start a scan session — runs the full recovery pipeline.
    pub async fn start_scan(
        &self,
        session_id: &SessionId,
        source_identity: &SourceIdentity,
    ) -> Result<()> {
        let mut session = {
            let db = self.db.lock().expect("db lock poisoned");
            db.load_session(session_id)?
        };

        if !session.source_identity.is_same_source(source_identity) {
            return Err(RecoverXError::SourceIdentityMismatch);
        }

        match session.status {
            SessionStatus::Running => {
                return Err(RecoverXError::ScanAlreadyRunning {
                    session_id: session_id.to_string(),
                });
            }
            SessionStatus::Completed | SessionStatus::Cancelled => {
                return Err(RecoverXError::Other(
                    "Session is already completed or cancelled".to_string(),
                ));
            }
            _ => {}
        }

        session.mark_running();
        self.persist_session(&session)?;

        self.event_bus
            .publish(recoverx_core::events::RecoverXEvent::ScanStarted(
                ScanStartedEvent {
                    session_id: session.id.clone(),
                    source_path: session.source_identity.path.clone(),
                    source_size: session.source_size_bytes,
                    scan_mode: session.configuration.mode.to_string(),
                    timestamp: Utc::now(),
                },
            ));

        let pipeline = build_pipeline(&session.id, &session.configuration);

        // Open the storage source once and share across tasks
        let source_path = session.source_identity.path.clone();
        let config = session.configuration.clone();

        // Run pipeline; collect all recovered files in memory
        let mut all_files: Vec<RecoveredFile> = Vec::new();
        let mut partitions: Vec<recoverx_engines::DetectedPartition> = Vec::new();
        let mut filesystems: Vec<recoverx_engines::DetectedFilesystem> = Vec::new();

        // ── SHORT-CIRCUIT: directory sources skip the raw disk pipeline ──────
        // When the user selects a folder (Downloads, Documents, Trash, etc.)
        // we walk it directly using the live filesystem instead of treating it
        // as a block device.
        if std::path::Path::new(&source_path).is_dir() {
            tracing::info!(session_id = %session.id, path = %source_path, "Directory source — using filesystem walker");

            let deleted_after = session.configuration.deleted_after;
            let deleted_before = session.configuration.deleted_before;

            let found = recoverx_engines::DirectoryScanner::scan(
                std::path::Path::new(&source_path),
                &session.id.to_string(),
                deleted_after,
                deleted_before,
            );

            tracing::info!(session_id = %session.id, count = found.len(), "Directory scan complete");

            for f in &found {
                self.event_bus.publish(recoverx_core::events::RecoverXEvent::FileFound(
                    recoverx_core::events::FileFoundEvent {
                        session_id: session.id.clone(),
                        file_id: f.id,
                        name: f.name.clone(),
                        size_bytes: f.size_bytes,
                        category: f.extension.as_deref().unwrap_or("unknown").to_string(),
                        confidence: f.confidence_label().to_string(),
                        recovery_method: f.recovery_method.to_string(),
                        timestamp: Utc::now(),
                    },
                ));
            }

            all_files = found;

            // Index and complete without running the raw pipeline
            let _ = self.task_index_results(&mut session, &all_files, &partitions).await;

            let db = self.db.lock().expect("db lock poisoned");
            let mut done = db.load_session(session_id)?;
            done.files_found = all_files.len() as u64;
            done.processed_bytes = session.source_size_bytes;
            done.mark_completed();
            db.update_session(&done)?;

            let duration = done.started_at
                .map(|s| (Utc::now() - s).num_seconds() as u64)
                .unwrap_or(0);

            self.event_bus.publish(recoverx_core::events::RecoverXEvent::ScanCompleted(
                ScanCompletedEvent {
                    session_id: session_id.clone(),
                    total_bytes_processed: done.processed_bytes,
                    files_found: done.files_found,
                    bad_sectors: 0,
                    duration_seconds: duration,
                    status: SessionStatus::Completed,
                    timestamp: Utc::now(),
                },
            ));

            return Ok(());
        }
        // ─────────────────────────────────────────────────────────────────────

        for mut task in pipeline {
            // Check for cancellation between tasks
            {
                let db = self.db.lock().expect("db lock poisoned");
                let current = db.load_session(session_id)?;
                if matches!(current.status, SessionStatus::Cancelled | SessionStatus::Failed) {
                    break;
                }
            }

            let result = self.run_task(
                &mut session,
                &mut task,
                &source_path,
                &config,
                &mut all_files,
                &mut partitions,
                &mut filesystems,
            ).await;

            if let Err(e) = result {
                session.mark_failed(e.to_string());
                self.persist_session(&session)?;
                self.event_bus.publish(recoverx_core::events::RecoverXEvent::ScanFailed(
                    ScanFailedEvent {
                        session_id: session_id.clone(),
                        error: e.to_string(),
                        last_offset: session.last_offset,
                        timestamp: Utc::now(),
                    },
                ));
                return Ok(());
            }
        }

        // Finalize
        let db = self.db.lock().expect("db lock poisoned");
        let final_session = db.load_session(session_id)?;

        if final_session.status == SessionStatus::Running {
            let mut done = final_session;
            done.files_found = all_files.len() as u64;
            done.mark_completed();
            db.update_session(&done)?;

            let duration = done
                .started_at
                .map(|s| (Utc::now() - s).num_seconds() as u64)
                .unwrap_or(0);

            self.event_bus.publish(recoverx_core::events::RecoverXEvent::ScanCompleted(
                ScanCompletedEvent {
                    session_id: session_id.clone(),
                    total_bytes_processed: done.processed_bytes,
                    files_found: done.files_found,
                    bad_sectors: done.bad_sectors,
                    duration_seconds: duration,
                    status: SessionStatus::Completed,
                    timestamp: Utc::now(),
                },
            ));
        }

        Ok(())
    }

    /// Pause a running scan.
    pub fn pause_scan(&self, session_id: &SessionId) -> Result<()> {
        let db = self.db.lock().expect("db lock poisoned");
        let mut session = db.load_session(session_id)?;

        if session.status != SessionStatus::Running {
            return Err(RecoverXError::Other(format!(
                "Cannot pause session in state {}",
                session.status
            )));
        }

        let last_offset = session.last_offset;
        session.mark_paused(last_offset);
        db.update_session(&session)?;

        self.event_bus.publish(recoverx_core::events::RecoverXEvent::ScanPaused(
            ScanPausedEvent {
                session_id: session_id.clone(),
                last_offset,
                timestamp: Utc::now(),
            },
        ));

        tracing::info!(session_id = %session_id, offset = last_offset, "Scan paused");
        Ok(())
    }

    /// Resume a paused scan.
    pub fn resume_scan(
        &self,
        session_id: &SessionId,
        current_identity: &SourceIdentity,
    ) -> Result<()> {
        let db = self.db.lock().expect("db lock poisoned");
        let mut session = db.load_session(session_id)?;

        if !session.is_resumable() {
            return Err(RecoverXError::Other(format!(
                "Session is not resumable (status: {})",
                session.status
            )));
        }

        if !session.source_identity.is_same_source(current_identity) {
            return Err(RecoverXError::SourceIdentityMismatch);
        }

        let resume_offset = session.last_offset;
        session.mark_running();
        db.update_session(&session)?;

        self.event_bus.publish(recoverx_core::events::RecoverXEvent::ScanResumed(
            ScanResumedEvent {
                session_id: session_id.clone(),
                resumed_from_offset: resume_offset,
                timestamp: Utc::now(),
            },
        ));

        tracing::info!(session_id = %session_id, offset = resume_offset, "Scan resumed");
        Ok(())
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn persist_session(&self, session: &ScanSession) -> Result<()> {
        let db = self.db.lock().expect("db lock poisoned");
        db.update_session(session)
    }

    fn emit_progress(&self, session: &ScanSession, stage: &str) {
        let percent = if session.source_size_bytes > 0 {
            (session.processed_bytes as f32 / session.source_size_bytes as f32 * 100.0).min(100.0)
        } else {
            0.0
        };
        self.event_bus.publish(recoverx_core::events::RecoverXEvent::ScanProgress(
            ScanProgressEvent {
                session_id: session.id.clone(),
                processed_bytes: session.processed_bytes,
                total_bytes: session.source_size_bytes,
                percent,
                speed_bps: 0,
                files_found: session.files_found,
                bad_sectors: session.bad_sectors,
                current_stage: stage.to_string(),
                eta_seconds: None,
                timestamp: Utc::now(),
            },
        ));
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_task(
        &self,
        session: &mut ScanSession,
        task: &mut PipelineTask,
        source_path: &str,
        config: &ScanConfiguration,
        all_files: &mut Vec<RecoveredFile>,
        partitions: &mut Vec<recoverx_engines::DetectedPartition>,
        filesystems: &mut Vec<recoverx_engines::DetectedFilesystem>,
    ) -> Result<()> {
        task.mark_running();

        self.event_bus.publish(recoverx_core::events::RecoverXEvent::TaskStateChanged(
            TaskStateChangedEvent {
                session_id: session.id.clone(),
                task_id: task.id.clone(),
                task_type: task.task_type.to_string(),
                old_status: TaskStatus::Pending,
                new_status: TaskStatus::Running,
                timestamp: Utc::now(),
            },
        ));

        self.emit_progress(session, &task.task_type.to_string());

        let result = match task.task_type {
            TaskType::SourceValidation => {
                self.task_source_validation(session, source_path).await
            }
            TaskType::PartitionDetection => {
                self.task_partition_detection(session, source_path, partitions).await
            }
            TaskType::FilesystemDetection => {
                self.task_filesystem_detection(session, source_path, partitions, filesystems).await
            }
            TaskType::FilesystemMetadataAnalysis => {
                if config.enable_filesystem_analysis {
                    self.task_filesystem_metadata(session, source_path, filesystems, all_files).await
                } else {
                    Ok(())
                }
            }
            TaskType::DeletedFileAnalysis => {
                if config.enable_deleted_file_recovery {
                    self.task_deleted_file_analysis(session, source_path, filesystems, all_files).await
                } else {
                    Ok(())
                }
            }
            TaskType::FileCarving => {
                if config.enable_file_carving {
                    self.task_file_carving(session, source_path, all_files).await
                } else {
                    Ok(())
                }
            }
            TaskType::ResultNormalization => {
                self.task_result_normalization(session, all_files).await
            }
            TaskType::DuplicateDetection => {
                if config.enable_duplicate_detection {
                    self.task_duplicate_detection(all_files).await
                } else {
                    Ok(())
                }
            }
            TaskType::ConfidenceScoring => {
                self.task_confidence_scoring(all_files).await
            }
            TaskType::IndexResults => {
                self.task_index_results(session, all_files, partitions).await
            }
        };

        match &result {
            Ok(()) => {
                task.mark_completed();
                self.event_bus.publish(recoverx_core::events::RecoverXEvent::TaskStateChanged(
                    TaskStateChangedEvent {
                        session_id: session.id.clone(),
                        task_id: task.id.clone(),
                        task_type: task.task_type.to_string(),
                        old_status: TaskStatus::Running,
                        new_status: TaskStatus::Completed,
                        timestamp: Utc::now(),
                    },
                ));
            }
            Err(e) => {
                task.mark_failed(e.to_string());
            }
        }

        result
    }

    // ── Individual task implementations ───────────────────────────────────

    async fn task_source_validation(&self, session: &mut ScanSession, source_path: &str) -> Result<()> {
        tracing::info!(session_id = %session.id, source = %source_path, "Source validation");
        // Verify the path exists (either device node or file)
        let exists = std::path::Path::new(source_path).exists();
        if !exists {
            return Err(RecoverXError::SourceNotFound { path: source_path.to_string() });
        }
        // Update size if we can determine it from the file
        if session.source_size_bytes == 0 {
            if let Ok(meta) = std::fs::metadata(source_path) {
                let sz = meta.len();
                if sz > 0 {
                    session.source_size_bytes = sz;
                    self.persist_session(session)?;
                }
            }
        }
        tracing::info!(session_id = %session.id, "Source validated: {} bytes", session.source_size_bytes);
        Ok(())
    }

    async fn task_partition_detection(
        &self,
        session: &mut ScanSession,
        source_path: &str,
        partitions: &mut Vec<recoverx_engines::DetectedPartition>,
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, "Partition detection");

        let mut source = open_source(source_path)?;
        match PartitionDetector::detect(source.as_mut()) {
            Ok(detected) => {
                tracing::info!(session_id = %session.id, count = detected.len(), "Partitions found");
                for p in &detected {
                    self.event_bus.publish(recoverx_core::events::RecoverXEvent::PartitionDetected(
                        PartitionDetectedEvent {
                            session_id: session.id.clone(),
                            partition_index: p.index,
                            start_offset: p.start_offset,
                            size_bytes: p.size_bytes,
                            partition_type: p.name.clone().unwrap_or_else(|| "Unknown".to_string()),
                            timestamp: Utc::now(),
                        },
                    ));
                }
                *partitions = detected;
            }
            Err(e) => {
                tracing::warn!(session_id = %session.id, "Partition detection failed: {}", e);
                // Non-fatal: if no partition table, treat as unpartitioned volume
            }
        }
        Ok(())
    }

    async fn task_filesystem_detection(
        &self,
        session: &mut ScanSession,
        source_path: &str,
        partitions: &[recoverx_engines::DetectedPartition],
        filesystems: &mut Vec<recoverx_engines::DetectedFilesystem>,
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, "Filesystem detection");

        let mut source = open_source(source_path)?;

        // If no partitions found, try to detect filesystem directly on the whole source
        let offsets_to_check: Vec<u64> = if partitions.is_empty() {
            vec![0]
        } else {
            partitions.iter().map(|p| p.start_offset).collect()
        };

        for offset in offsets_to_check {
            match FilesystemDetector::detect(source.as_mut(), offset) {
                Ok(fs) => {
                    tracing::info!(
                        session_id = %session.id,
                        fs_type = %fs.fs_type,
                        offset = offset,
                        "Filesystem detected"
                    );
                    self.event_bus.publish(recoverx_core::events::RecoverXEvent::FilesystemDetected(
                        FilesystemDetectedEvent {
                            session_id: session.id.clone(),
                            partition_index: partitions.iter()
                                .find(|p| p.start_offset == offset)
                                .map(|p| p.index),
                            filesystem_type: fs.fs_type.to_string(),
                            label: fs.label.clone(),
                            timestamp: Utc::now(),
                        },
                    ));
                    filesystems.push(fs);
                }
                Err(e) => {
                    tracing::warn!(session_id = %session.id, offset = offset, "FS detection error: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn task_filesystem_metadata(
        &self,
        session: &mut ScanSession,
        source_path: &str,
        filesystems: &[recoverx_engines::DetectedFilesystem],
        all_files: &mut Vec<RecoveredFile>,
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, "Filesystem metadata analysis");

        for fs in filesystems {
            let source = open_source(source_path)?;
            let _session_id_str = session.id.to_string();
            let offset = fs.offset_in_source;

            use recoverx_engines::FilesystemType;
            match fs.fs_type {
                FilesystemType::Fat32 | FilesystemType::ExFat => {
                    // FAT32 deleted file scan handles metadata + deleted entries
                    // This pass does the live (non-deleted) metadata
                    tracing::info!(session_id = %session.id, offset = offset, "FAT32 metadata scan");
                    // FAT32Analyzer handles both live and deleted entries
                }
                FilesystemType::Ntfs => {
                    tracing::info!(session_id = %session.id, offset = offset, "NTFS metadata scan");
                    // NTFS MFT parsing done in deleted_file_analysis
                }
                FilesystemType::Ext2 | FilesystemType::Ext3 | FilesystemType::Ext4 => {
                    tracing::info!(session_id = %session.id, offset = offset, "ext4 metadata scan");
                }
                FilesystemType::Apfs => {
                    tracing::info!(session_id = %session.id, offset = offset, "APFS metadata (read-only, limited)");
                }
                _ => {
                    tracing::debug!(session_id = %session.id, "Skipping metadata for {:?}", fs.fs_type);
                }
            }

            session.update_progress(
                (offset + fs.total_size / 4).min(session.source_size_bytes),
                all_files.len() as u64,
                session.bad_sectors,
                (offset + fs.total_size / 4).min(session.source_size_bytes),
            );
            self.persist_session(session)?;
            let _ = source; // explicitly drop
        }

        Ok(())
    }

    async fn task_deleted_file_analysis(
        &self,
        session: &mut ScanSession,
        source_path: &str,
        filesystems: &[recoverx_engines::DetectedFilesystem],
        all_files: &mut Vec<RecoveredFile>,
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, "Deleted file analysis");

        let session_id_str = session.id.to_string();
        let max_per_fs = 50_000usize;

        for fs in filesystems {
            let mut source = open_source(source_path)?;
            let offset = fs.offset_in_source;

            use recoverx_engines::FilesystemType;
            let found = match fs.fs_type {
                FilesystemType::Fat32 | FilesystemType::ExFat => {
                    tracing::info!(session_id = %session.id, offset = offset, "FAT32 deleted entry scan");
                    match Fat32Analyzer::analyze(source.as_mut(), offset, &session_id_str, max_per_fs) {
                        Ok(files) => {
                            tracing::info!(session_id = %session.id, count = files.len(), "FAT32 deleted files found");
                            files
                        }
                        Err(e) => {
                            tracing::warn!(session_id = %session.id, "FAT32 analysis failed: {}", e);
                            vec![]
                        }
                    }
                }
                FilesystemType::Ntfs => {
                    tracing::info!(session_id = %session.id, offset = offset, "NTFS MFT scan");
                    match recoverx_engines::filesystem::NtfsAnalyzer::analyze(
                        source.as_mut(), offset, &session_id_str, max_per_fs
                    ) {
                        Ok(files) => {
                            tracing::info!(session_id = %session.id, count = files.len(), "NTFS files found");
                            files
                        }
                        Err(e) => {
                            tracing::warn!(session_id = %session.id, "NTFS analysis failed: {}", e);
                            vec![]
                        }
                    }
                }
                FilesystemType::Ext2 | FilesystemType::Ext3 | FilesystemType::Ext4 => {
                    tracing::info!(session_id = %session.id, offset = offset, "ext4 inode scan");
                    match recoverx_engines::filesystem::Ext4Analyzer::analyze(
                        source.as_mut(), offset, &session_id_str, max_per_fs
                    ) {
                        Ok(files) => {
                            tracing::info!(session_id = %session.id, count = files.len(), "ext4 files found");
                            files
                        }
                        Err(e) => {
                            tracing::warn!(session_id = %session.id, "ext4 analysis failed: {}", e);
                            vec![]
                        }
                    }
                }
                _ => {
                    tracing::debug!(session_id = %session.id, "No deleted-entry scanner for {:?}", fs.fs_type);
                    vec![]
                }
            };

            // Emit file-found events
            for f in &found {
                self.event_bus.publish(recoverx_core::events::RecoverXEvent::FileFound(
                    recoverx_core::events::FileFoundEvent {
                        session_id: session.id.clone(),
                        file_id: f.id,
                        name: f.name.clone(),
                        size_bytes: f.size_bytes,
                        category: f.extension.as_deref().unwrap_or("unknown").to_string(),
                        confidence: f.confidence_label().to_string(),
                        recovery_method: f.recovery_method.to_string(),
                        timestamp: Utc::now(),
                    },
                ));
            }

            all_files.extend(found);
        }

        // Also scan Trash/.Trashes paths
        self.task_trash_recovery(session, source_path, filesystems, all_files).await?;

        session.update_progress(
            session.source_size_bytes / 2,
            all_files.len() as u64,
            session.bad_sectors,
            session.source_size_bytes / 2,
        );
        self.persist_session(session)?;

        Ok(())
    }

    async fn task_trash_recovery(
        &self,
        session: &mut ScanSession,
        source_path: &str,
        _filesystems: &[recoverx_engines::DetectedFilesystem],
        all_files: &mut Vec<RecoveredFile>,
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, "Trash/Recycle Bin recovery");

        // Scan host filesystem Trash paths if source is a mounted volume
        let paths_to_check = vec![
            // macOS
            format!("{}/.Trashes", source_path),
            "/Users".to_string(), // will glob for ~/.Trash
            // Linux
            format!("{}/.local/share/Trash", source_path),
        ];

        let session_id_str = session.id.to_string();

        for trash_path in &paths_to_check {
            let p = std::path::Path::new(trash_path);
            if p.exists() && p.is_dir() {
                let found = recoverx_engines::trash::TrashScanner::scan_path(
                    p,
                    &session_id_str,
                );
                tracing::info!(
                    session_id = %session.id,
                    path = %trash_path,
                    count = found.len(),
                    "Trash entries found"
                );
                for f in &found {
                    self.event_bus.publish(recoverx_core::events::RecoverXEvent::FileFound(
                        recoverx_core::events::FileFoundEvent {
                            session_id: session.id.clone(),
                            file_id: f.id,
                            name: f.name.clone(),
                            size_bytes: f.size_bytes,
                            category: f.extension.as_deref().unwrap_or("unknown").to_string(),
                            confidence: f.confidence_label().to_string(),
                            recovery_method: f.recovery_method.to_string(),
                            timestamp: Utc::now(),
                        },
                    ));
                }
                all_files.extend(found);
            }
        }

        Ok(())
    }

    async fn task_file_carving(
        &self,
        session: &mut ScanSession,
        source_path: &str,
        all_files: &mut Vec<RecoveredFile>,
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, "File carving");

        let session_id_str = session.id.to_string();
        let mut source = open_source(source_path)?;

        let carver = FileCarver::new();
        match carver.carve(source.as_mut(), &session_id_str, session.source_size_bytes) {
            Ok(files) => {
                tracing::info!(session_id = %session.id, count = files.len(), "Carved files found");

                for f in &files {
                    self.event_bus.publish(recoverx_core::events::RecoverXEvent::FileFound(
                        recoverx_core::events::FileFoundEvent {
                            session_id: session.id.clone(),
                            file_id: f.id,
                            name: f.name.clone(),
                            size_bytes: f.size_bytes,
                            category: f.extension.as_deref().unwrap_or("unknown").to_string(),
                            confidence: f.confidence_label().to_string(),
                            recovery_method: f.recovery_method.to_string(),
                            timestamp: Utc::now(),
                        },
                    ));
                }

                all_files.extend(files);
            }
            Err(e) => {
                tracing::warn!(session_id = %session.id, "File carving failed: {}", e);
            }
        }

        session.update_progress(
            (session.source_size_bytes * 3) / 4,
            all_files.len() as u64,
            session.bad_sectors,
            (session.source_size_bytes * 3) / 4,
        );
        self.persist_session(session)?;

        Ok(())
    }

    async fn task_result_normalization(
        &self,
        session: &mut ScanSession,
        all_files: &mut Vec<RecoveredFile>,
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, before = all_files.len(), "Result normalization");

        // Apply timeline filter from scan configuration
        let deleted_after = session.configuration.deleted_after;
        let deleted_before = session.configuration.deleted_before;

        if deleted_after.is_some() || deleted_before.is_some() {
            all_files.retain(|f| {
                // For Trash files, use modified_at as deletion time proxy
                // For carved files with no timestamp, keep them (can't filter what we don't know)
                let ts = f.modified_at.map(|t| t.timestamp())
                    .or_else(|| f.created_at.map(|t| t.timestamp()));

                match ts {
                    None => true, // no timestamp — keep
                    Some(t) => {
                        let after_ok = deleted_after.map_or(true, |a| t >= a);
                        let before_ok = deleted_before.map_or(true, |b| t <= b);
                        after_ok && before_ok
                    }
                }
            });
            tracing::info!(
                session_id = %session.id,
                after_filter = all_files.len(),
                "Timeline filter applied"
            );
        }

        // Deduplicate by source_offset — keep highest-confidence entry per offset
        use std::collections::HashMap;
        let mut by_offset: HashMap<u64, usize> = HashMap::new();

        for (i, f) in all_files.iter().enumerate() {
            let entry = by_offset.entry(f.source_offset).or_insert(i);
            if all_files[*entry].confidence < f.confidence {
                *entry = i;
            }
        }

        let keep: std::collections::HashSet<usize> = by_offset.into_values().collect();
        let mut idx = 0usize;
        all_files.retain(|_| {
            let keep_it = keep.contains(&idx);
            idx += 1;
            keep_it
        });

        tracing::info!(session_id = %session.id, after = all_files.len(), "Normalization complete");
        Ok(())
    }

    async fn task_duplicate_detection(
        &self,
        all_files: &mut Vec<RecoveredFile>,
    ) -> Result<()> {
        // Mark files with same size + name as potential duplicates (lightweight)
        // Full SHA-256 dedup happens at recovery time to avoid reading all data
        use std::collections::HashMap;
        let mut seen: HashMap<(u64, String), usize> = HashMap::new();
        for (i, f) in all_files.iter_mut().enumerate() {
            let key = (f.size_bytes, f.name.clone());
            if seen.contains_key(&key) {
                // Reduce confidence slightly for likely duplicates
                f.confidence = f.confidence.saturating_sub(5);
            } else {
                seen.insert(key, i);
            }
        }
        Ok(())
    }

    async fn task_confidence_scoring(
        &self,
        all_files: &mut Vec<RecoveredFile>,
    ) -> Result<()> {
        for f in all_files.iter_mut() {
            // Boost confidence for files with original paths
            if f.original_path.is_some() {
                f.confidence = (f.confidence + 10).min(100);
            }
            // Boost for recognized extensions
            if let Some(ext) = &f.extension {
                if is_known_extension(ext) {
                    f.confidence = (f.confidence + 5).min(100);
                }
            }
            // Reduce for fragmented files
            if f.is_fragmented {
                f.confidence = f.confidence.saturating_sub(10);
            }
        }
        Ok(())
    }

    async fn task_index_results(
        &self,
        session: &mut ScanSession,
        all_files: &[RecoveredFile],
        partitions: &[recoverx_engines::DetectedPartition],
    ) -> Result<()> {
        tracing::info!(session_id = %session.id, count = all_files.len(), "Indexing results");

        let db = self.db.lock().expect("db lock poisoned");

        // Clear any previous results for this session
        db.clear_recovered_files(&session.id.to_string())?;

        // Insert partitions
        for p in partitions {
            let _ = db.insert_partition(&session.id.to_string(), p);
        }

        // Insert all recovered files
        for f in all_files {
            if let Err(e) = db.insert_recovered_file(f) {
                tracing::warn!(session_id = %session.id, file = %f.name, "Failed to index file: {}", e);
            }
        }

        // Update session counters
        let mut updated = db.load_session(&session.id)?;
        updated.files_found = all_files.len() as u64;
        updated.processed_bytes = session.source_size_bytes;
        updated.last_offset = session.source_size_bytes;
        db.update_session(&updated)?;

        tracing::info!(session_id = %session.id, files = all_files.len(), "Results indexed");
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Open a StorageSource for the given path.
/// Tries physical device first, falls back to disk image file.
fn open_source(path: &str) -> Result<Box<dyn recoverx_storage::source::StorageSource>> {
    // Physical device paths
    if path.starts_with("/dev/") || path.starts_with(r"\\.\") {
        match PhysicalDeviceSource::open(path) {
            Ok(src) => return Ok(Box::new(src)),
            Err(e) => {
                tracing::warn!("Cannot open {} as physical device: {}", path, e);
                // Fall through to try as image
            }
        }
    }

    // File-based disk image
    match DiskImageSource::open(path) {
        Ok(src) => Ok(Box::new(src)),
        Err(e) => Err(RecoverXError::SourceNotReadable {
            reason: format!("Cannot open '{}': {}", path, e),
        }),
    }
}

fn is_known_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "heic"
            | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "csv" | "rtf"
            | "zip" | "rar" | "7z" | "tar" | "gz"
            | "mp3" | "wav" | "flac" | "ogg" | "m4a"
            | "mp4" | "mov" | "avi" | "mkv" | "mpeg"
            | "sqlite" | "db"
            | "html" | "css" | "js" | "json" | "xml"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use recoverx_core::{identity::SourceIdentity, types::ScanConfiguration};
    use tempfile::TempDir;

    fn make_identity(path: &str, size: u64) -> SourceIdentity {
        SourceIdentity {
            label: path.to_string(),
            path: path.to_string(),
            size_bytes: size,
            sector_size: 512,
            model: None,
            serial: Some("TEST001".to_string()),
            filesystem_label: None,
            filesystem_type: None,
            partial_hash: None,
        }
    }

    #[tokio::test]
    async fn create_and_load_session() {
        let dir = TempDir::new().unwrap();
        let orchestrator = RecoveryOrchestrator::new(dir.path().to_path_buf()).unwrap();

        let identity = make_identity("/dev/test0", 1_000_000);
        let config = ScanConfiguration::quick();
        let session = orchestrator.create_session(identity, config).unwrap();

        let loaded = orchestrator.load_session(&session.id).unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.status, SessionStatus::Created);
    }

    #[tokio::test]
    async fn cancel_session_persists() {
        let dir = TempDir::new().unwrap();
        let orchestrator = RecoveryOrchestrator::new(dir.path().to_path_buf()).unwrap();

        let identity = make_identity("/dev/test0", 1_000_000);
        let config = ScanConfiguration::quick();
        let session = orchestrator.create_session(identity, config).unwrap();

        orchestrator.cancel_session(&session.id).unwrap();

        let loaded = orchestrator.load_session(&session.id).unwrap();
        assert_eq!(loaded.status, SessionStatus::Cancelled);
    }

    #[tokio::test]
    async fn identity_mismatch_rejects_start() {
        let dir = TempDir::new().unwrap();
        let orchestrator = RecoveryOrchestrator::new(dir.path().to_path_buf()).unwrap();

        let identity = make_identity("/dev/test0", 1_000_000);
        let wrong_identity = make_identity("/dev/test0", 2_000_000);
        let config = ScanConfiguration::quick();
        let session = orchestrator.create_session(identity, config).unwrap();

        let result = orchestrator.start_scan(&session.id, &wrong_identity).await;
        assert!(matches!(result, Err(RecoverXError::SourceIdentityMismatch)));
    }
}
