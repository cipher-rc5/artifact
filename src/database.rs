//! Redb-backed deletion history store.
//!
//! Schema: a primary `deletions` table keyed by auto-incremented `u64` ID,
//! with secondary indices on `(deleted_at, id)` for time-range scans and
//! `(dir_type, id)` for type-grouped queries.

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use rkyv::{Archive, Deserialize, Serialize, rancor::Error as RkyvError, util::AlignedVec};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, instrument};

use crate::directory_item::DirectoryType;
use crate::error::{ArtifactError, Result};

const DB_FILE: &str = "artifact.redb";
const SCHEMA_VERSION: i64 = 1;

// Primary table: id -> rkyv-archived DeletionRecord
const RECORDS: TableDefinition<u64, &[u8]> = TableDefinition::new("deletions");

// Secondary index for time-range scans: (deleted_at, id) -> ()
// Composite key keeps entries unique even when timestamps collide.
const IDX_DELETED_AT: TableDefinition<(i64, u64), ()> = TableDefinition::new("idx_deleted_at");

// Secondary index for dir_type grouping: (dir_type, id) -> ()
const IDX_DIR_TYPE: TableDefinition<(&str, u64), ()> = TableDefinition::new("idx_dir_type");

// Single-row table holding the next id to assign.
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const META_NEXT_ID: &str = "next_id";

/// A record of a single directory deletion, persisted to the redb database.
///
/// Created via [`DeletionRecord::new`] before insertion; the `id` field is
/// assigned by [`DeletionDatabase::record_deletion`] on first write.
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
pub struct DeletionRecord {
    id: i64,
    pub path: String,
    pub dir_type: String,
    pub size_bytes: i64,
    pub project_root: Option<String>,
    pub project_name: Option<String>,
    pub deleted_at: i64,
    pub metadata: String,
}

impl DeletionRecord {
    /// Create a new unperisted deletion record.
    ///
    /// The `id` is `0` until the record is written to the database via
    /// [`DeletionDatabase::record_deletion`], which returns the assigned ID.
    pub fn new(
        path: PathBuf,
        dir_type: DirectoryType,
        size_bytes: u64,
        project_root: Option<PathBuf>,
        project_name: Option<String>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let hostname_val = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .map(|s| {
                // Minimal JSON string escaping for hostname
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            })
            .unwrap_or_else(|| "null".to_string());

        let metadata = format!(
            r#"{{"version":{},"hostname":{}}}"#,
            SCHEMA_VERSION,
            hostname_val
        );

        Self {
            id: 0,
            path: path.to_string_lossy().to_string(),
            dir_type: dir_type.name().to_string(),
            size_bytes: size_bytes as i64,
            project_root: project_root.map(|p| p.to_string_lossy().to_string()),
            project_name,
            deleted_at: now,
            metadata,
        }
    }

    /// Return the database-assigned integer ID.
    ///
    /// Returns `0` for records that have not yet been persisted via
    /// [`DeletionDatabase::record_deletion`].
    pub fn id(&self) -> i64 {
        self.id
    }
}

pub struct DeletionDatabase {
    db: Arc<Database>,
}

impl DeletionDatabase {
    /// Open (or create) the deletion database at the given directory.
    ///
    /// If `data_dir` is `None`, the database is placed in the platform config
    /// directory (`~/.config/artifact/` on Linux, `~/Library/Application Support/artifact/` on macOS).
    /// The required directory is created if it does not exist.
    #[instrument(skip_all)]
    pub fn new(data_dir: Option<PathBuf>) -> Result<Self> {
        info!("Initializing deletion database");

        let db_path = if let Some(dir) = data_dir {
            std::fs::create_dir_all(&dir).map_err(|e| {
                ArtifactError::DatabaseInit(format!("Could not create data directory: {}", e))
            })?;
            dir.join(DB_FILE)
        } else {
            let config_dir = dirs::config_dir()
                .ok_or_else(|| {
                    ArtifactError::Configuration("Could not find config directory".to_string())
                })?
                .join("artifact");

            std::fs::create_dir_all(&config_dir).map_err(|e| {
                ArtifactError::DatabaseInit(format!("Could not create config directory: {}", e))
            })?;

            config_dir.join(DB_FILE)
        };

        debug!("Database path: {}", db_path.display());

        let db = Database::create(&db_path)
            .map_err(|e| ArtifactError::DatabaseConnection(e.to_string()))?;

        let instance = Self { db: Arc::new(db) };
        instance.initialize_schema()?;

        info!("Database initialized successfully");
        Ok(instance)
    }

    #[instrument(skip(self))]
    fn initialize_schema(&self) -> Result<()> {
        debug!("Initializing database schema");

        let write_txn = self.db.begin_write()?;
        // Opening each table inside a write txn creates them on first use.
        let _ = write_txn.open_table(RECORDS)?;
        let _ = write_txn.open_table(IDX_DELETED_AT)?;
        let _ = write_txn.open_table(IDX_DIR_TYPE)?;
        let _ = write_txn.open_table(META)?;
        write_txn.commit()?;

        debug!("Schema initialized successfully");
        Ok(())
    }

    /// Persist a deletion record and return the assigned integer ID.
    ///
    /// The record is written to three tables atomically: the primary records
    /// table, the time-index, and the type-index.
    #[instrument(skip(self, record), fields(path = %record.path))]
    pub fn record_deletion(&self, record: &DeletionRecord) -> Result<i64> {
        debug!(
            "Recording deletion: {} ({} bytes)",
            record.path, record.size_bytes
        );

        let write_txn = self.db.begin_write()?;
        let new_id: u64 = {
            let mut meta = write_txn.open_table(META)?;
            let next = meta.get(META_NEXT_ID)?.map(|v| v.value()).unwrap_or(1);
            meta.insert(META_NEXT_ID, next + 1)?;
            next
        };

        let stored = DeletionRecord {
            id: new_id as i64,
            ..record.clone()
        };
        let bytes = Self::encode_record(&stored)?;

        {
            let mut records = write_txn.open_table(RECORDS)?;
            records.insert(new_id, bytes.as_slice())?;
        }
        {
            let mut idx_time = write_txn.open_table(IDX_DELETED_AT)?;
            idx_time.insert((stored.deleted_at, new_id), ())?;
        }
        {
            let mut idx_type = write_txn.open_table(IDX_DIR_TYPE)?;
            idx_type.insert((stored.dir_type.as_str(), new_id), ())?;
        }
        write_txn.commit()?;

        info!("Deletion recorded with ID: {}", new_id);
        Ok(new_id as i64)
    }

    fn encode_record(record: &DeletionRecord) -> Result<AlignedVec<16>> {
        rkyv::to_bytes::<RkyvError>(record)
            .map_err(|e| ArtifactError::DatabaseQuery(format!("encode: {}", e)))
    }

    // rkyv requires the buffer to satisfy the archive's alignment, but slices
    // borrowed from redb make no such guarantee, so copy into an AlignedVec first.
    fn decode_record(bytes: &[u8]) -> Result<DeletionRecord> {
        let mut aligned = AlignedVec::<16>::with_capacity(bytes.len());
        aligned.extend_from_slice(bytes);
        rkyv::from_bytes::<DeletionRecord, RkyvError>(&aligned)
            .map_err(|e| ArtifactError::DatabaseQuery(format!("decode: {}", e)))
    }

    fn load_record(
        records: &impl ReadableTable<u64, &'static [u8]>,
        id: u64,
    ) -> Result<Option<DeletionRecord>> {
        let Some(value) = records.get(id)? else {
            return Ok(None);
        };
        Ok(Some(Self::decode_record(value.value())?))
    }

    /// Return up to `limit` deletion records ordered newest-first.
    #[instrument(skip(self))]
    pub fn get_recent_deletions(&self, limit: usize) -> Result<Vec<DeletionRecord>> {
        debug!("Fetching {} recent deletions", limit);

        let read_txn = self.db.begin_read()?;
        let idx_time = read_txn.open_table(IDX_DELETED_AT)?;
        let records = read_txn.open_table(RECORDS)?;

        let mut out = Vec::with_capacity(limit);
        // iter().rev() walks descending by (deleted_at, id) so newest first.
        for entry in idx_time.iter()?.rev() {
            if out.len() >= limit {
                break;
            }
            let (key, _) = entry?;
            let (_, id) = key.value();
            if let Some(rec) = Self::load_record(&records, id)? {
                out.push(rec);
            }
        }

        debug!("Retrieved {} deletion records", out.len());
        Ok(out)
    }

    /// Return deletion records whose `deleted_at` Unix timestamp falls within
    /// `[start_timestamp, end_timestamp]`, ordered newest-first.
    #[instrument(skip(self))]
    pub fn get_deletions_by_time_range(
        &self,
        start_timestamp: i64,
        end_timestamp: i64,
    ) -> Result<Vec<DeletionRecord>> {
        debug!(
            "Fetching deletions between {} and {}",
            start_timestamp, end_timestamp
        );

        let read_txn = self.db.begin_read()?;
        let idx_time = read_txn.open_table(IDX_DELETED_AT)?;
        let records = read_txn.open_table(RECORDS)?;

        let lo = (start_timestamp, u64::MIN);
        let hi = (end_timestamp, u64::MAX);

        let mut out = Vec::new();
        for entry in idx_time.range(lo..=hi)?.rev() {
            let (key, _) = entry?;
            let (_, id) = key.value();
            if let Some(rec) = Self::load_record(&records, id)? {
                out.push(rec);
            }
        }

        info!("Retrieved {} deletions in time range", out.len());
        Ok(out)
    }

    /// Sum the `size_bytes` of every deletion record and return the total.
    #[instrument(skip(self))]
    pub fn get_total_space_freed(&self) -> Result<i64> {
        debug!("Calculating total space freed");

        let read_txn = self.db.begin_read()?;
        let records = read_txn.open_table(RECORDS)?;

        let mut total: i64 = 0;
        for entry in records.iter()? {
            let (_, value) = entry?;
            let rec = Self::decode_record(value.value())?;
            total += rec.size_bytes;
        }

        info!("Total space freed: {} bytes", total);
        Ok(total)
    }

    /// Compute aggregate statistics over all deletion records.
    #[instrument(skip(self))]
    pub fn get_deletion_statistics(&self) -> Result<DeletionStatistics> {
        debug!("Calculating deletion statistics");

        let read_txn = self.db.begin_read()?;
        let records = read_txn.open_table(RECORDS)?;

        let mut total_deletions: i64 = 0;
        let mut total_space_freed: i64 = 0;
        let mut by_type: std::collections::HashMap<String, (i64, i64)> =
            std::collections::HashMap::new();

        for entry in records.iter()? {
            let (_, value) = entry?;
            let rec = Self::decode_record(value.value())?;
            total_deletions += 1;
            total_space_freed += rec.size_bytes;
            let entry = by_type.entry(rec.dir_type.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += rec.size_bytes;
        }

        let stats = DeletionStatistics {
            total_deletions,
            total_space_freed,
            deletions_by_type: by_type,
        };

        info!("Statistics calculated: {:?}", stats);
        Ok(stats)
    }

    /// Delete records older than `older_than_days` days and return the count removed.
    ///
    /// Pass a negative value (e.g. `-1`) to remove all records regardless of age.
    #[instrument(skip(self))]
    pub fn cleanup_old_records(&self, older_than_days: i64) -> Result<usize> {
        let cutoff_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - (older_than_days * 86400);

        info!(
            "Cleaning up records older than {} days (timestamp: {})",
            older_than_days, cutoff_timestamp
        );

        let write_txn = self.db.begin_write()?;
        let mut removed: usize = 0;

        let stale_keys: Vec<(i64, u64)> = {
            let idx_time = write_txn.open_table(IDX_DELETED_AT)?;
            let lo = (i64::MIN, u64::MIN);
            let hi = (cutoff_timestamp - 1, u64::MAX);
            idx_time
                .range(lo..=hi)?
                .map(|res| res.map(|(k, _)| k.value()))
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        {
            let mut records = write_txn.open_table(RECORDS)?;
            let mut idx_time = write_txn.open_table(IDX_DELETED_AT)?;
            let mut idx_type = write_txn.open_table(IDX_DIR_TYPE)?;

            for (ts, id) in stale_keys {
                if let Some(value) = records.get(id)? {
                    let rec = Self::decode_record(value.value())?;
                    drop(value);
                    idx_type.remove((rec.dir_type.as_str(), id))?;
                }
                records.remove(id)?;
                idx_time.remove((ts, id))?;
                removed += 1;
            }
        }

        write_txn.commit()?;

        info!("Cleaned up {} old records", removed);
        Ok(removed)
    }
}

/// Aggregate statistics computed over all deletion records.
#[derive(Debug, Clone)]
pub struct DeletionStatistics {
    /// Total number of deletion records.
    pub total_deletions: i64,
    /// Sum of `size_bytes` across all records.
    pub total_space_freed: i64,
    /// Per-type breakdown: maps `dir_type` name → `(count, total_bytes)`.
    pub deletions_by_type: std::collections::HashMap<String, (i64, i64)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_item::DirectoryType;
    use crate::rules;

    fn temp_db() -> (DeletionDatabase, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = DeletionDatabase::new(Some(tmp.path().to_path_buf())).unwrap();
        (db, tmp)
    }

    fn sample_record() -> DeletionRecord {
        let rule = rules::find("node_modules").unwrap();
        DeletionRecord::new(
            std::path::PathBuf::from("/tmp/myproject/node_modules"),
            DirectoryType::new(rule),
            512 * 1024 * 1024, // 512 MiB
            Some(std::path::PathBuf::from("/tmp/myproject")),
            Some("myproject".to_string()),
        )
    }

    #[test]
    fn insert_and_retrieve() {
        let (db, _tmp) = temp_db();
        let record = sample_record();
        let id = db.record_deletion(&record).unwrap();
        assert!(id > 0);

        let recent = db.get_recent_deletions(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].path, record.path);
        assert_eq!(recent[0].size_bytes, record.size_bytes);
    }

    #[test]
    fn recent_deletions_ordered_newest_first() {
        let (db, _tmp) = temp_db();
        let r1 = sample_record();
        let r2 = DeletionRecord::new(
            std::path::PathBuf::from("/tmp/other/node_modules"),
            DirectoryType::new(rules::find("node_modules").unwrap()),
            1024,
            None,
            Some("other".to_string()),
        );
        db.record_deletion(&r1).unwrap();
        // Small sleep to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.record_deletion(&r2).unwrap();

        let recent = db.get_recent_deletions(10).unwrap();
        assert_eq!(recent.len(), 2);
        // Newest (r2) should come first
        assert!(recent[0].deleted_at >= recent[1].deleted_at);
    }

    #[test]
    fn statistics_sums_correctly() {
        let (db, _tmp) = temp_db();
        db.record_deletion(&sample_record()).unwrap();
        db.record_deletion(&sample_record()).unwrap();

        let stats = db.get_deletion_statistics().unwrap();
        assert_eq!(stats.total_deletions, 2);
        assert_eq!(stats.total_space_freed, 2 * (512 * 1024 * 1024));
        assert!(stats.deletions_by_type.contains_key("node_modules"));
    }

    #[test]
    fn cleanup_old_records_removes_stale() {
        let (db, _tmp) = temp_db();
        db.record_deletion(&sample_record()).unwrap();

        // Passing -1 days means "older than yesterday" which is everything ever
        // inserted (since records are at most seconds old). Use a negative
        // older_than_days to force cleanup of all records.
        let removed = db.cleanup_old_records(-1).unwrap();
        assert_eq!(removed, 1);

        let recent = db.get_recent_deletions(10).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn empty_db_returns_empty_results() {
        let (db, _tmp) = temp_db();
        let recent = db.get_recent_deletions(10).unwrap();
        assert!(recent.is_empty());
        let stats = db.get_deletion_statistics().unwrap();
        assert_eq!(stats.total_deletions, 0);
        assert_eq!(stats.total_space_freed, 0);
    }
}
