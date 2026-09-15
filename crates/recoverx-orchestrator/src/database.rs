//! SQLite persistence for scan sessions.
//!
//! Schema is created on first open (migration 1).  Future migrations append
//! new `ALTER TABLE` statements with the next version number.

use std::path::Path;

use chrono::{DateTime, Utc};
use recoverx_core::{
    error::{RecoverXError, Result},
    identity::SourceIdentity,
    types::{ScanConfiguration, SessionId, SessionStatus},
};
use rusqlite::{params, Connection};

use crate::session::ScanSession;

/// Helper: convert rusqlite error to RecoverXError::Database.
fn db_err(e: rusqlite::Error) -> RecoverXError {
    RecoverXError::Database(e.to_string())
}

/// Manages the SQLite database for RecoverX.
pub struct SessionDatabase {
    conn: Connection,
}

impl SessionDatabase {
    /// Open (or create) the database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref()).map_err(db_err)?;

        // Enable WAL mode for better concurrent read performance
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(db_err)?;

        let mut db = SessionDatabase { conn };
        db.run_migrations()?;
        Ok(db)
    }

    /// Open an in-memory database (used for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(db_err)?;
        let mut db = SessionDatabase { conn };
        db.run_migrations()?;
        Ok(db)
    }

    // ── Schema migrations ─────────────────────────────────────────────────

    fn run_migrations(&mut self) -> Result<()> {
        self.conn
            .execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")
            .map_err(db_err)?;

        let version: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if version < 1 {
            self.migrate_v1()?;
        }
        if version < 2 {
            self.migrate_v2()?;
        }

        Ok(())
    }

    fn migrate_v1(&mut self) -> Result<()> {
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS scan_sessions (
                id                  TEXT PRIMARY KEY NOT NULL,
                source_identity     TEXT NOT NULL,       -- JSON
                source_size_bytes   INTEGER NOT NULL,
                sector_size         INTEGER NOT NULL,
                configuration       TEXT NOT NULL,       -- JSON
                status              TEXT NOT NULL,
                last_offset         INTEGER NOT NULL DEFAULT 0,
                processed_bytes     INTEGER NOT NULL DEFAULT 0,
                files_found         INTEGER NOT NULL DEFAULT 0,
                bad_sectors         INTEGER NOT NULL DEFAULT 0,
                engine_state        TEXT,                -- JSON, nullable
                last_error          TEXT,
                created_at          TEXT NOT NULL,
                updated_at          TEXT NOT NULL,
                started_at          TEXT,
                completed_at        TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_status
                ON scan_sessions(status);
            CREATE INDEX IF NOT EXISTS idx_sessions_created
                ON scan_sessions(created_at DESC);
            ",
            )
            .map_err(|e| RecoverXError::DatabaseMigration(format!("v1: {}", e)))?;

        self.conn
            .execute("INSERT INTO schema_version (version) VALUES (1)", [])
            .map_err(db_err)?;

        Ok(())
    }

    fn migrate_v2(&mut self) -> Result<()> {
        self.conn
            .execute_batch(
                "
            -- Recovered file records
            CREATE TABLE IF NOT EXISTS recovered_files (
                id                  TEXT PRIMARY KEY NOT NULL,   -- UUID
                session_id          TEXT NOT NULL,
                name                TEXT NOT NULL,
                original_path       TEXT,
                size_bytes          INTEGER NOT NULL DEFAULT 0,
                source_offset       INTEGER NOT NULL DEFAULT 0,
                partition_index     INTEGER,
                filesystem_type     TEXT,
                recovery_method     TEXT NOT NULL,
                confidence          INTEGER NOT NULL DEFAULT 0,
                status              TEXT NOT NULL,
                sha256              TEXT,
                created_at_file     TEXT,
                modified_at_file    TEXT,
                accessed_at_file    TEXT,
                extension           TEXT,
                mime_type           TEXT,
                is_deleted          INTEGER NOT NULL DEFAULT 1,
                is_fragmented       INTEGER NOT NULL DEFAULT 0,
                fragments_json      TEXT,
                metadata_json       TEXT,
                FOREIGN KEY (session_id) REFERENCES scan_sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_recovered_session
                ON recovered_files(session_id);
            CREATE INDEX IF NOT EXISTS idx_recovered_extension
                ON recovered_files(extension);
            CREATE INDEX IF NOT EXISTS idx_recovered_status
                ON recovered_files(status);
            CREATE INDEX IF NOT EXISTS idx_recovered_confidence
                ON recovered_files(confidence DESC);

            -- Detected partition records
            CREATE TABLE IF NOT EXISTS detected_partitions (
                id                      INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id              TEXT NOT NULL,
                partition_index         INTEGER NOT NULL,
                partition_table_type    TEXT NOT NULL,
                type_guid               TEXT,
                type_id                 INTEGER,
                start_lba               INTEGER NOT NULL DEFAULT 0,
                end_lba                 INTEGER NOT NULL DEFAULT 0,
                start_offset            INTEGER NOT NULL DEFAULT 0,
                size_bytes              INTEGER NOT NULL DEFAULT 0,
                name                    TEXT,
                filesystem              TEXT,
                is_bootable             INTEGER NOT NULL DEFAULT 0,
                is_active               INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (session_id) REFERENCES scan_sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_partitions_session
                ON detected_partitions(session_id);

            -- Bad sector records
            CREATE TABLE IF NOT EXISTS bad_sectors (
                id              TEXT PRIMARY KEY NOT NULL,   -- UUID
                session_id      TEXT NOT NULL,
                sector_number   INTEGER NOT NULL,
                byte_offset     INTEGER NOT NULL,
                error_message   TEXT NOT NULL,
                detected_at     TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES scan_sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_bad_sectors_session
                ON bad_sectors(session_id);

            -- Recovery operation log
            CREATE TABLE IF NOT EXISTS recovery_log (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id          TEXT NOT NULL,
                file_id             TEXT NOT NULL,
                destination_path    TEXT NOT NULL,
                status              TEXT NOT NULL,
                sha256              TEXT,
                size_recovered      INTEGER NOT NULL DEFAULT 0,
                error_message       TEXT,
                recovered_at        TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES scan_sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_recovery_log_session
                ON recovery_log(session_id);
            ",
            )
            .map_err(|e| RecoverXError::DatabaseMigration(format!("v2: {}", e)))?;

        self.conn
            .execute("INSERT INTO schema_version (version) VALUES (2)", [])
            .map_err(db_err)?;

        Ok(())
    }

    // ── CRUD ──────────────────────────────────────────────────────────────

    /// Insert a new scan session.
    pub fn insert_session(&self, session: &ScanSession) -> Result<()> {
        let identity_json = serde_json::to_string(&session.source_identity)?;
        let config_json = serde_json::to_string(&session.configuration)?;
        let engine_state_json = session
            .engine_state
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        self.conn
            .execute(
                "INSERT INTO scan_sessions (
                id, source_identity, source_size_bytes, sector_size,
                configuration, status, last_offset, processed_bytes,
                files_found, bad_sectors, engine_state, last_error,
                created_at, updated_at, started_at, completed_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    session.id.to_string(),
                    identity_json,
                    session.source_size_bytes as i64,
                    session.sector_size as i64,
                    config_json,
                    session.status.to_string(),
                    session.last_offset as i64,
                    session.processed_bytes as i64,
                    session.files_found as i64,
                    session.bad_sectors as i64,
                    engine_state_json,
                    session.last_error,
                    session.created_at.to_rfc3339(),
                    session.updated_at.to_rfc3339(),
                    session.started_at.map(|t| t.to_rfc3339()),
                    session.completed_at.map(|t| t.to_rfc3339()),
                ],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// Update an existing scan session.
    pub fn update_session(&self, session: &ScanSession) -> Result<()> {
        let identity_json = serde_json::to_string(&session.source_identity)?;
        let config_json = serde_json::to_string(&session.configuration)?;
        let engine_state_json = session
            .engine_state
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        let rows = self
            .conn
            .execute(
                "UPDATE scan_sessions SET
                source_identity     = ?2,
                source_size_bytes   = ?3,
                sector_size         = ?4,
                configuration       = ?5,
                status              = ?6,
                last_offset         = ?7,
                processed_bytes     = ?8,
                files_found         = ?9,
                bad_sectors         = ?10,
                engine_state        = ?11,
                last_error          = ?12,
                updated_at          = ?13,
                started_at          = ?14,
                completed_at        = ?15
             WHERE id = ?1",
                params![
                    session.id.to_string(),
                    identity_json,
                    session.source_size_bytes as i64,
                    session.sector_size as i64,
                    config_json,
                    session.status.to_string(),
                    session.last_offset as i64,
                    session.processed_bytes as i64,
                    session.files_found as i64,
                    session.bad_sectors as i64,
                    engine_state_json,
                    session.last_error,
                    session.updated_at.to_rfc3339(),
                    session.started_at.map(|t| t.to_rfc3339()),
                    session.completed_at.map(|t| t.to_rfc3339()),
                ],
            )
            .map_err(db_err)?;

        if rows == 0 {
            return Err(RecoverXError::SessionNotFound {
                id: session.id.to_string(),
            });
        }
        Ok(())
    }

    /// Load a single session by ID.
    pub fn load_session(&self, id: &SessionId) -> Result<ScanSession> {
        self.conn
            .query_row(
                "SELECT * FROM scan_sessions WHERE id = ?1",
                params![id.to_string()],
                row_to_session,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    RecoverXError::SessionNotFound { id: id.to_string() }
                }
                other => db_err(other),
            })
    }

    /// List all sessions, ordered by creation time descending.
    pub fn list_sessions(&self) -> Result<Vec<ScanSession>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM scan_sessions ORDER BY created_at DESC")
            .map_err(db_err)?;
        let rows = stmt.query_map([], row_to_session).map_err(db_err)?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(db_err)?);
        }
        Ok(sessions)
    }

    /// Delete a session and all associated data.
    pub fn delete_session(&self, id: &SessionId) -> Result<()> {
        let rows = self
            .conn
            .execute(
                "DELETE FROM scan_sessions WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(db_err)?;
        if rows == 0 {
            return Err(RecoverXError::SessionNotFound { id: id.to_string() });
        }
        Ok(())
    }

    // ── Recovered files ───────────────────────────────────────────────────

    /// Insert a recovered file record.
    pub fn insert_recovered_file(&self, file: &recoverx_engines::RecoveredFile) -> Result<()> {
        let fragments_json = serde_json::to_string(&file.fragments).unwrap_or_default();
        let metadata_json = serde_json::to_string(&file.metadata).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO recovered_files (
                id, session_id, name, original_path, size_bytes, source_offset,
                partition_index, filesystem_type, recovery_method, confidence, status,
                sha256, created_at_file, modified_at_file, accessed_at_file,
                extension, mime_type, is_deleted, is_fragmented, fragments_json, metadata_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            params![
                file.id.to_string(),
                file.session_id,
                file.name,
                file.original_path,
                file.size_bytes as i64,
                file.source_offset as i64,
                file.partition_index.map(|v| v as i64),
                file.filesystem_type,
                file.recovery_method.to_string(),
                file.confidence as i64,
                file.status.to_string(),
                file.sha256,
                file.created_at.map(|t| t.to_rfc3339()),
                file.modified_at.map(|t| t.to_rfc3339()),
                file.accessed_at.map(|t| t.to_rfc3339()),
                file.extension,
                file.mime_type,
                file.is_deleted as i64,
                file.is_fragmented as i64,
                fragments_json,
                metadata_json,
            ],
        ).map_err(db_err)?;
        Ok(())
    }

    /// List all recovered files for a session.
    pub fn list_recovered_files(&self, session_id: &str) -> Result<Vec<recoverx_engines::RecoveredFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM recovered_files WHERE session_id = ?1 ORDER BY confidence DESC, name ASC"
        ).map_err(db_err)?;

        let rows = stmt.query_map(params![session_id], |row| {
            use recoverx_engines::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};
            use uuid::Uuid;

            let id_str: String = row.get("id")?;
            let fragments_json: String = row.get("fragments_json").unwrap_or_default();
            let metadata_json: String = row.get("metadata_json").unwrap_or_default();

            let fragments: Vec<FileFragment> = serde_json::from_str(&fragments_json).unwrap_or_default();
            let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap_or(serde_json::Value::Null);

            let status_str: String = row.get("status")?;
            let status = match status_str.as_str() {
                "complete" | "Complete" => FileStatus::Complete,
                "partial" | "Partial" => FileStatus::Partial,
                "fragmented" | "Fragmented" => FileStatus::Fragmented,
                "corrupted" | "Corrupted" => FileStatus::Corrupted,
                "encrypted" | "Encrypted" => FileStatus::Encrypted,
                _ => FileStatus::Partial,
            };

            let method_str: String = row.get("recovery_method")?;
            let recovery_method = match method_str.as_str() {
                "MFT Record" => RecoveryMethod::MftRecord,
                "Inode Record" => RecoveryMethod::InodeRecord,
                "Directory Entry" => RecoveryMethod::DirectoryEntry,
                "File Carving" => RecoveryMethod::FileCarving,
                "Hybrid Reconstruction" => RecoveryMethod::HybridReconstruction,
                _ => RecoveryMethod::FilesystemMetadata,
            };

            let parse_dt = |s: Option<String>| -> Option<chrono::DateTime<chrono::Utc>> {
                s.and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
                 .map(|dt| dt.with_timezone(&chrono::Utc))
            };

            Ok(RecoveredFile {
                id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                session_id: row.get("session_id")?,
                name: row.get("name")?,
                original_path: row.get("original_path")?,
                size_bytes: row.get::<_, i64>("size_bytes")? as u64,
                source_offset: row.get::<_, i64>("source_offset")? as u64,
                partition_index: row.get::<_, Option<i64>>("partition_index")?.map(|v| v as u32),
                filesystem_type: row.get("filesystem_type")?,
                recovery_method,
                confidence: row.get::<_, i64>("confidence")? as u8,
                status,
                sha256: row.get("sha256")?,
                created_at: parse_dt(row.get("created_at_file")?),
                modified_at: parse_dt(row.get("modified_at_file")?),
                accessed_at: parse_dt(row.get("accessed_at_file")?),
                extension: row.get("extension")?,
                mime_type: row.get("mime_type")?,
                is_deleted: row.get::<_, i64>("is_deleted")? != 0,
                is_fragmented: row.get::<_, i64>("is_fragmented")? != 0,
                fragments,
                metadata,
            })
        }).map_err(db_err)?;

        let mut files = Vec::new();
        for row in rows {
            files.push(row.map_err(db_err)?);
        }
        Ok(files)
    }

    /// Paginated, filtered query for the Results UI.
    pub fn query_recovered_files(
        &self,
        session_id: &str,
        category: Option<&str>,   // extension category filter
        search: Option<&str>,     // filename search
        limit: i64,
        offset: i64,
    ) -> Result<Vec<recoverx_engines::RecoveredFile>> {
        // Build extension list for category
        let ext_list = category.map(category_extensions);

        let mut sql = format!(
            "SELECT * FROM recovered_files WHERE session_id = ?1"
        );

        if ext_list.is_some() {
            sql.push_str(" AND LOWER(extension) IN (SELECT value FROM json_each(?2))");
        }
        if search.is_some() {
            let param = if ext_list.is_some() { "?3" } else { "?2" };
            sql.push_str(&format!(" AND LOWER(name) LIKE {}", param));
        }
        sql.push_str(" ORDER BY confidence DESC, name ASC LIMIT ?");
        sql.push_str(&format!(" OFFSET {}", offset));

        // Build params dynamically
        let mut stmt = self.conn.prepare(&sql).map_err(db_err)?;

        let ext_json = ext_list.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        let search_pattern = search.map(|s| format!("%{}%", s.to_lowercase()));

        let param_count = 1
            + if ext_json.is_some() { 1 } else { 0 }
            + if search_pattern.is_some() { 1 } else { 0 }
            + 1; // limit

        // Use rusqlite's dynamic param approach
        let rows: Vec<recoverx_engines::RecoveredFile> = match (ext_json.as_deref(), search_pattern.as_deref()) {
            (None, None) => {
                stmt.query_map(params![session_id, limit], row_to_recovered_file)
                    .map_err(db_err)?
                    .filter_map(|r| r.ok())
                    .collect()
            }
            (Some(ext), None) => {
                stmt.query_map(params![session_id, ext, limit], row_to_recovered_file)
                    .map_err(db_err)?
                    .filter_map(|r| r.ok())
                    .collect()
            }
            (None, Some(s)) => {
                stmt.query_map(params![session_id, s, limit], row_to_recovered_file)
                    .map_err(db_err)?
                    .filter_map(|r| r.ok())
                    .collect()
            }
            (Some(ext), Some(s)) => {
                stmt.query_map(params![session_id, ext, s, limit], row_to_recovered_file)
                    .map_err(db_err)?
                    .filter_map(|r| r.ok())
                    .collect()
            }
        };

        Ok(rows)
    }

    /// Get counts per category for the tab bar.
    pub fn category_counts(&self, session_id: &str) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT LOWER(COALESCE(extension, '')) as ext, COUNT(*) as cnt
             FROM recovered_files WHERE session_id = ?1
             GROUP BY ext"
        ).map_err(db_err)?;

        let rows = stmt.query_map(params![session_id], |row| {
            let ext: String = row.get(0)?;
            let cnt: i64 = row.get(1)?;
            Ok((ext, cnt))
        }).map_err(db_err)?;

        let mut ext_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut total: i64 = 0;

        for row in rows.flatten() {
            let cat = ext_to_category(&row.0);
            *ext_counts.entry(cat).or_insert(0) += row.1;
            total += row.1;
        }

        let mut result: Vec<(String, i64)> = ext_counts.into_iter().collect();
        result.sort_by(|a, b| b.1.cmp(&a.1));
        result.insert(0, ("All".to_string(), total));
        Ok(result)
    }

    /// Delete all recovered files for a session (before re-indexing).
    pub fn clear_recovered_files(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM recovered_files WHERE session_id = ?1",
            params![session_id],
        ).map_err(db_err)?;
        Ok(())
    }

    // ── Partitions ────────────────────────────────────────────────────────

    /// Insert a detected partition record.
    pub fn insert_partition(&self, session_id: &str, p: &recoverx_engines::DetectedPartition) -> Result<()> {
        self.conn.execute(
            "INSERT INTO detected_partitions (
                session_id, partition_index, partition_table_type, type_guid, type_id,
                start_lba, end_lba, start_offset, size_bytes, name, filesystem,
                is_bootable, is_active
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                session_id,
                p.index as i64,
                p.partition_table_type.to_string(),
                p.type_guid,
                p.type_id.map(|v| v as i64),
                p.start_lba as i64,
                p.end_lba as i64,
                p.start_offset as i64,
                p.size_bytes as i64,
                p.name,
                p.filesystem,
                p.is_bootable as i64,
                p.is_active as i64,
            ],
        ).map_err(db_err)?;
        Ok(())
    }

    /// List partitions for a session.
    pub fn list_partitions(&self, session_id: &str) -> Result<Vec<recoverx_engines::DetectedPartition>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM detected_partitions WHERE session_id = ?1 ORDER BY partition_index ASC"
        ).map_err(db_err)?;

        let rows = stmt.query_map(params![session_id], |row| {
            use recoverx_engines::{DetectedPartition, PartitionTableType};
            let pt_str: String = row.get("partition_table_type")?;
            let pt = match pt_str.as_str() {
                "GPT" => PartitionTableType::Gpt,
                "MBR" => PartitionTableType::Mbr,
                "APFS Container" => PartitionTableType::ApfsContainer,
                _ => PartitionTableType::Unknown,
            };
            Ok(DetectedPartition {
                index: row.get::<_, i64>("partition_index")? as u32,
                partition_table_type: pt,
                type_guid: row.get("type_guid")?,
                type_id: row.get::<_, Option<i64>>("type_id")?.map(|v| v as u8),
                start_lba: row.get::<_, i64>("start_lba")? as u64,
                end_lba: row.get::<_, i64>("end_lba")? as u64,
                start_offset: row.get::<_, i64>("start_offset")? as u64,
                size_bytes: row.get::<_, i64>("size_bytes")? as u64,
                name: row.get("name")?,
                filesystem: row.get("filesystem")?,
                is_bootable: row.get::<_, i64>("is_bootable")? != 0,
                is_active: row.get::<_, i64>("is_active")? != 0,
            })
        }).map_err(db_err)?;

        let mut parts = Vec::new();
        for row in rows {
            parts.push(row.map_err(db_err)?);
        }
        Ok(parts)
    }
}

// ── Row deserialisation ───────────────────────────────────────────────────────

fn row_to_recovered_file(row: &rusqlite::Row) -> rusqlite::Result<recoverx_engines::RecoveredFile> {
    use recoverx_engines::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};
    use uuid::Uuid;

    let id_str: String = row.get("id")?;
    let fragments_json: String = row.get("fragments_json").unwrap_or_default();
    let metadata_json: String = row.get("metadata_json").unwrap_or_default();
    let fragments: Vec<FileFragment> = serde_json::from_str(&fragments_json).unwrap_or_default();
    let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
        .unwrap_or(serde_json::Value::Null);

    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "complete" | "Complete" => FileStatus::Complete,
        "partial" | "Partial" => FileStatus::Partial,
        "fragmented" | "Fragmented" => FileStatus::Fragmented,
        "corrupted" | "Corrupted" => FileStatus::Corrupted,
        "encrypted" | "Encrypted" => FileStatus::Encrypted,
        _ => FileStatus::Partial,
    };
    let method_str: String = row.get("recovery_method")?;
    let recovery_method = match method_str.as_str() {
        "MFT Record" => RecoveryMethod::MftRecord,
        "Inode Record" => RecoveryMethod::InodeRecord,
        "Directory Entry" => RecoveryMethod::DirectoryEntry,
        "File Carving" => RecoveryMethod::FileCarving,
        "Hybrid Reconstruction" => RecoveryMethod::HybridReconstruction,
        _ => RecoveryMethod::FilesystemMetadata,
    };
    let parse_dt = |s: Option<String>| -> Option<chrono::DateTime<chrono::Utc>> {
        s.and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
         .map(|dt| dt.with_timezone(&chrono::Utc))
    };
    Ok(RecoveredFile {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        session_id: row.get("session_id")?,
        name: row.get("name")?,
        original_path: row.get("original_path")?,
        size_bytes: row.get::<_, i64>("size_bytes")? as u64,
        source_offset: row.get::<_, i64>("source_offset")? as u64,
        partition_index: row.get::<_, Option<i64>>("partition_index")?.map(|v| v as u32),
        filesystem_type: row.get("filesystem_type")?,
        recovery_method,
        confidence: row.get::<_, i64>("confidence")? as u8,
        status,
        sha256: row.get("sha256")?,
        created_at: parse_dt(row.get("created_at_file")?),
        modified_at: parse_dt(row.get("modified_at_file")?),
        accessed_at: parse_dt(row.get("accessed_at_file")?),
        extension: row.get("extension")?,
        mime_type: row.get("mime_type")?,
        is_deleted: row.get::<_, i64>("is_deleted")? != 0,
        is_fragmented: row.get::<_, i64>("is_fragmented")? != 0,
        fragments,
        metadata,
    })
}

/// Map an extension to a display category name.
fn ext_to_category(ext: &str) -> String {
    match ext {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "heic" | "svg" | "ico" | "raw" => "Images",
        "mp4" | "mov" | "avi" | "mkv" | "mpeg" | "mpg" | "wmv" | "flv" | "m4v" | "webm" => "Videos",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" | "aiff" => "Audio",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "csv" | "pages" | "numbers" | "key" | "odt" | "ods" | "odp" => "Documents",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "dmg" | "pkg" | "deb" | "rpm" => "Archives",
        "sqlite" | "db" | "sql" | "mdb" | "accdb" => "Databases",
        "html" | "htm" | "css" | "js" | "ts" | "jsx" | "tsx" | "json" | "xml" | "yaml" | "yml" | "toml" | "md" | "rs" | "py" | "swift" | "java" | "kt" | "go" | "cpp" | "c" | "h" => "Code",
        "" => "Other",
        _ => "Other",
    }.to_string()
}

/// Return a JSON array of extensions for a given category name.
fn category_extensions(category: &str) -> Vec<String> {
    match category {
        "Images" => vec!["jpg","jpeg","png","gif","bmp","tiff","webp","heic","svg","ico","raw"],
        "Videos" => vec!["mp4","mov","avi","mkv","mpeg","mpg","wmv","flv","m4v","webm"],
        "Audio"  => vec!["mp3","wav","flac","ogg","m4a","aac","wma","aiff"],
        "Documents" => vec!["pdf","doc","docx","xls","xlsx","ppt","pptx","txt","rtf","csv","pages","numbers","key","odt","ods","odp"],
        "Archives" => vec!["zip","rar","7z","tar","gz","bz2","xz","dmg","pkg","deb","rpm"],
        "Databases" => vec!["sqlite","db","sql","mdb","accdb"],
        "Code" => vec!["html","htm","css","js","ts","jsx","tsx","json","xml","yaml","yml","toml","md","rs","py","swift","java","kt","go","cpp","c","h"],
        _ => vec![],
    }.into_iter().map(|s| s.to_string()).collect()
}

fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<ScanSession> {
    let id_str: String = row.get("id")?;
    let identity_json: String = row.get("source_identity")?;
    let config_json: String = row.get("configuration")?;
    let status_str: String = row.get("status")?;
    let engine_state_json: Option<String> = row.get("engine_state")?;
    let created_at_str: String = row.get("created_at")?;
    let updated_at_str: String = row.get("updated_at")?;
    let started_at_str: Option<String> = row.get("started_at")?;
    let completed_at_str: Option<String> = row.get("completed_at")?;

    let id = id_str.parse::<SessionId>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let source_identity: SourceIdentity = serde_json::from_str(&identity_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let configuration: ScanConfiguration = serde_json::from_str(&config_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status = parse_session_status(&status_str);

    let engine_state = engine_state_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .ok()
        .flatten();

    let parse_dt = |s: &str| -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    };

    let created_at = parse_dt(&created_at_str).unwrap_or_else(Utc::now);
    let updated_at = parse_dt(&updated_at_str).unwrap_or_else(Utc::now);
    let started_at = started_at_str.as_deref().and_then(parse_dt);
    let completed_at = completed_at_str.as_deref().and_then(parse_dt);

    Ok(ScanSession {
        id,
        source_size_bytes: row.get::<_, i64>("source_size_bytes")? as u64,
        sector_size: row.get::<_, i64>("sector_size")? as u32,
        source_identity,
        configuration,
        status,
        last_offset: row.get::<_, i64>("last_offset")? as u64,
        processed_bytes: row.get::<_, i64>("processed_bytes")? as u64,
        files_found: row.get::<_, i64>("files_found")? as u64,
        bad_sectors: row.get::<_, i64>("bad_sectors")? as u64,
        engine_state,
        last_error: row.get("last_error")?,
        created_at,
        updated_at,
        started_at,
        completed_at,
    })
}

fn parse_session_status(s: &str) -> SessionStatus {
    match s {
        "Created" => SessionStatus::Created,
        "Running" => SessionStatus::Running,
        "Paused" => SessionStatus::Paused,
        "Completed" => SessionStatus::Completed,
        "Failed" => SessionStatus::Failed,
        "Cancelled" => SessionStatus::Cancelled,
        _ => SessionStatus::Failed,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use recoverx_core::{identity::SourceIdentity, types::ScanConfiguration};

    fn make_identity() -> SourceIdentity {
        SourceIdentity {
            label: "test-disk".to_string(),
            path: "/dev/test0".to_string(),
            size_bytes: 2_000_000,
            sector_size: 512,
            model: Some("TestDisk 2000".to_string()),
            serial: Some("TD2000SN001".to_string()),
            filesystem_label: None,
            filesystem_type: None,
            partial_hash: None,
        }
    }

    #[test]
    fn insert_and_load_session() {
        let db = SessionDatabase::open_in_memory().unwrap();
        let session = ScanSession::new(make_identity(), ScanConfiguration::quick());
        let id = session.id.clone();

        db.insert_session(&session).unwrap();
        let loaded = db.load_session(&id).unwrap();

        assert_eq!(loaded.id, id);
        assert_eq!(loaded.status, SessionStatus::Created);
        assert_eq!(
            loaded.source_identity.serial.as_deref(),
            Some("TD2000SN001")
        );
    }

    #[test]
    fn update_session_progress() {
        let db = SessionDatabase::open_in_memory().unwrap();
        let mut session = ScanSession::new(make_identity(), ScanConfiguration::deep());
        let id = session.id.clone();

        db.insert_session(&session).unwrap();

        session.mark_running();
        session.update_progress(1_000_000, 42, 3, 1_000_000);
        db.update_session(&session).unwrap();

        let loaded = db.load_session(&id).unwrap();
        assert_eq!(loaded.status, SessionStatus::Running);
        assert_eq!(loaded.files_found, 42);
        assert_eq!(loaded.bad_sectors, 3);
        assert_eq!(loaded.last_offset, 1_000_000);
    }

    #[test]
    fn list_sessions_returns_all() {
        let db = SessionDatabase::open_in_memory().unwrap();
        for _ in 0..3 {
            db.insert_session(&ScanSession::new(
                make_identity(),
                ScanConfiguration::quick(),
            ))
            .unwrap();
        }
        assert_eq!(db.list_sessions().unwrap().len(), 3);
    }

    #[test]
    fn delete_session_removes_it() {
        let db = SessionDatabase::open_in_memory().unwrap();
        let session = ScanSession::new(make_identity(), ScanConfiguration::quick());
        let id = session.id.clone();
        db.insert_session(&session).unwrap();
        db.delete_session(&id).unwrap();
        assert!(db.load_session(&id).is_err());
    }

    #[test]
    fn load_nonexistent_session_returns_error() {
        let db = SessionDatabase::open_in_memory().unwrap();
        let id = SessionId::new();
        assert!(db.load_session(&id).is_err());
    }
}
