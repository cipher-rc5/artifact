//! Redb-backed deletion history store.
//!
//! Schema: a primary `deletions` table keyed by auto-incremented `u64` ID,
//! with secondary indices on `(deleted_at, id)` for time-range scans and
//! `(dir_type, id)` for type-grouped queries.
//!
//! Records are serialized with `rkyv`. On read, each row is copied into an
//! aligned buffer and **fully deserialized** into an owned [`DeletionRecord`]
//! (see [`DeletionDatabase::decode_record`]) — this is not a zero-copy access
//! of the archived form. The copy is intentional: slices borrowed from redb
//! carry no alignment guarantee, so they cannot be reinterpreted in place.

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use rkyv::{Archive, Deserialize, Serialize, rancor::Error as RkyvError, util::AlignedVec};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, instrument, warn};

use crate::directory_item::DirectoryType;
use crate::error::{ArtifactError, Result};

const DB_FILE: &str = "artifact.redb";

/// Current on-disk schema version. Bump this and add a migration arm in
/// [`DeletionDatabase::run_migrations`] whenever the persisted layout changes.
const SCHEMA_VERSION: u64 = 1;

/// Metadata embedded in every [`DeletionRecord`], serialized to JSON via serde.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RecordMetadata {
    version: u64,
    hostname: Option<String>,
}

/// Map an init-phase redb error to [`ArtifactError::DatabaseInit`].
///
/// Query paths rely on the blanket `From<redb::Error>` (which yields
/// `DatabaseQuery`); schema setup/migration uses this instead so that a
/// corrupt/incompatible store reports as an initialization failure.
fn init_err<E: std::fmt::Display>(e: E) -> ArtifactError {
    ArtifactError::DatabaseInit(e.to_string())
}

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
const META_SCHEMA_VERSION: &str = "schema_version";

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
        // `duration_since` fails only if the clock is before 1970 (VM restore,
        // dead RTC battery). Treat that as epoch rather than panicking.
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let hostname_val = hostname::get().ok().and_then(|h| h.into_string().ok());

        // serde_json handles all escaping (control chars, quotes, unicode)
        // correctly. Serialization of this fixed struct cannot fail, but fall
        // back to an empty object rather than unwrap on the impossible case.
        let metadata = serde_json::to_string(&RecordMetadata {
            version: SCHEMA_VERSION,
            hostname: hostname_val,
        })
        .unwrap_or_else(|_| "{}".to_string());

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
    /// If `data_dir` is `None`, the database is placed under the platform data
    /// directory (`~/.local/share/artifact/db/` on Linux,
    /// `~/Library/Application Support/artifact/db/` on macOS,
    /// `%APPDATA%\artifact\db\` on Windows). This mirrors
    /// [`crate::config::AppConfig::get_db_path`] so the default location is
    /// identical whether the caller passes an explicit path or `None`.
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
            // Single-sourced with AppConfig::get_db_path(): platform data dir,
            // `artifact/db` subdirectory — not the config dir (fixes the M5
            // divergence where None fell back to a different location).
            let data_dir = dirs::data_dir()
                .ok_or_else(|| {
                    ArtifactError::Configuration("Could not find data directory".to_string())
                })?
                .join("artifact")
                .join("db");

            std::fs::create_dir_all(&data_dir).map_err(|e| {
                ArtifactError::DatabaseInit(format!("Could not create data directory: {}", e))
            })?;

            data_dir.join(DB_FILE)
        };

        debug!("Database path: {}", db_path.display());

        // Peek the stored schema version (if any) *before* opening for real, so
        // we can take a backup ahead of a potential migration or rejection.
        let stored_version = Self::peek_schema_version(&db_path);
        let needs_backup =
            db_path.exists() && stored_version.map(|v| v != SCHEMA_VERSION).unwrap_or(false);
        if needs_backup {
            Self::backup_before_migration(&db_path, stored_version);
        }

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

        // Init-phase redb failures are wrapped as `DatabaseInit` (via
        // `init_err`) so they surface to the user as "could not initialize
        // database" rather than the generic "database query failed" produced by
        // the blanket `From<redb::Error>` used on the query paths.
        let write_txn = self.db.begin_write().map_err(init_err)?;
        // Opening each table inside a write txn creates them on first use.
        write_txn.open_table(RECORDS).map_err(init_err)?;
        write_txn.open_table(IDX_DELETED_AT).map_err(init_err)?;
        write_txn.open_table(IDX_DIR_TYPE).map_err(init_err)?;
        {
            let mut meta = write_txn.open_table(META).map_err(init_err)?;
            let stored_version = meta
                .get(META_SCHEMA_VERSION)
                .map_err(init_err)?
                .map(|v| v.value());
            match stored_version {
                // Fresh database: stamp the current version.
                None => {
                    meta.insert(META_SCHEMA_VERSION, SCHEMA_VERSION)
                        .map_err(init_err)?;
                }
                // Already at the current version: nothing to do.
                Some(version) if version == SCHEMA_VERSION => {}
                // Older, but known: run forward migrations in order.
                Some(version) if version < SCHEMA_VERSION => {
                    Self::run_migrations(&mut meta, version)?;
                }
                // Newer than this build supports: refuse rather than risk
                // misreading a future layout.
                Some(version) => {
                    return Err(ArtifactError::DatabaseInit(format!(
                        "Database schema version {version} is newer than supported \
                         (this build supports up to {SCHEMA_VERSION}); \
                         please upgrade the application"
                    )));
                }
            }
        }
        write_txn.commit().map_err(init_err)?;

        debug!("Schema initialized successfully");
        Ok(())
    }

    /// Apply forward migrations to bring an on-disk database from `from_version`
    /// up to [`SCHEMA_VERSION`].
    ///
    /// This is deliberately structured as an ordered, fall-through ladder so
    /// that new versions only require appending a single arm (e.g. a
    /// `v1 -> v2` step) without touching the earlier ones. Each step should be
    /// idempotent where possible and must leave the stored `schema_version`
    /// equal to [`SCHEMA_VERSION`] on success.
    ///
    /// Callers are expected to have taken a backup of the database file (see
    /// [`DeletionDatabase::backup_before_migration`]) before invoking this.
    fn run_migrations(meta: &mut redb::Table<'_, &str, u64>, from_version: u64) -> Result<()> {
        let mut version = from_version;
        info!(
            "Migrating database schema from version {} to {}",
            from_version, SCHEMA_VERSION
        );

        // Ordered migration ladder. Add future steps here, e.g.:
        //   if version == 1 { migrate_v1_to_v2(meta)?; version = 2; }
        //
        // The current build only knows version 1, so there is no pre-1 layout
        // to migrate from in practice; this branch exists so the structure is
        // ready the moment SCHEMA_VERSION is bumped.
        if version == 0 {
            // v0 predates the versioned `meta` table; nothing structural to do
            // beyond adopting the current version stamp.
            debug!("Applying migration v0 -> v1");
            version = 1;
        }

        if version != SCHEMA_VERSION {
            return Err(ArtifactError::DatabaseInit(format!(
                "No migration path from schema version {from_version} to {SCHEMA_VERSION}"
            )));
        }

        meta.insert(META_SCHEMA_VERSION, SCHEMA_VERSION)
            .map_err(init_err)?;
        info!(
            "Schema migration complete (now at version {})",
            SCHEMA_VERSION
        );
        Ok(())
    }

    /// Best-effort read of the stored schema version from an existing redb
    /// file, without disturbing it. Returns `None` if the file is absent,
    /// unopenable, has no `meta` table, or has no version stamp.
    fn peek_schema_version(db_path: &std::path::Path) -> Option<u64> {
        if !db_path.exists() {
            return None;
        }
        let db = Database::open(db_path).ok()?;
        let read_txn = db.begin_read().ok()?;
        let meta = read_txn.open_table(META).ok()?;
        meta.get(META_SCHEMA_VERSION).ok()?.map(|v| v.value())
    }

    /// Copy the redb file to a timestamped `<file>.bak-<version>-<epoch>` sibling
    /// before an open that may migrate or reject it. Best-effort: a failure to
    /// back up is logged but does not abort the open, since the original file is
    /// never mutated by the copy itself.
    fn backup_before_migration(db_path: &std::path::Path, stored_version: Option<u64>) {
        if !db_path.exists() {
            return;
        }
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let version_tag = stored_version
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let mut backup = db_path.as_os_str().to_owned();
        backup.push(format!(".bak-{version_tag}-{epoch}"));
        let backup = PathBuf::from(backup);
        match std::fs::copy(db_path, &backup) {
            Ok(_) => info!("Backed up database to {}", backup.display()),
            Err(e) => warn!("Could not back up database before open: {}", e),
        }
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

    // This is a full deserialize, not a zero-copy access: rkyv requires the
    // buffer to satisfy the archive's alignment, but slices borrowed from redb
    // make no such guarantee, so we copy into an AlignedVec first and then
    // materialize an owned `DeletionRecord`. The copy is intentional.
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
            match Self::load_record(&records, id)? {
                Some(rec) => out.push(rec),
                None => warn!(
                    id,
                    "dangling time-index entry: no record found for indexed id"
                ),
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
            match Self::load_record(&records, id)? {
                Some(rec) => out.push(rec),
                None => warn!(
                    id,
                    "dangling time-index entry: no record found for indexed id"
                ),
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
            let (key, value) = entry?;
            // A single undecodable row must not abort the whole aggregation;
            // skip it and warn rather than returning an error.
            match Self::decode_record(value.value()) {
                Ok(rec) => total += rec.size_bytes,
                Err(e) => warn!(
                    id = key.value(),
                    error = %e,
                    "skipping corrupt deletion record while summing space freed"
                ),
            }
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
            let (key, value) = entry?;
            // Skip-and-warn on a corrupt row so one bad record cannot poison
            // the entire statistics computation.
            let rec = match Self::decode_record(value.value()) {
                Ok(rec) => rec,
                Err(e) => {
                    warn!(
                        id = key.value(),
                        error = %e,
                        "skipping corrupt deletion record while computing statistics"
                    );
                    continue;
                }
            };
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
            .unwrap_or_default()
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

    /// Stamp a redb file at `dir` with an explicit schema version, leaving all
    /// schema tables present so it looks like a real ARTIFACT database.
    fn stamp_schema_version(dir: &std::path::Path, version: u64) {
        let db_path = dir.join(DB_FILE);
        let raw = Database::create(&db_path).unwrap();
        let txn = raw.begin_write().unwrap();
        {
            txn.open_table(RECORDS).unwrap();
            txn.open_table(IDX_DELETED_AT).unwrap();
            txn.open_table(IDX_DIR_TYPE).unwrap();
            let mut meta = txn.open_table(META).unwrap();
            meta.insert(META_SCHEMA_VERSION, version).unwrap();
        }
        txn.commit().unwrap();
        drop(raw);
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        // A version newer than this build supports must still be rejected,
        // now with the "newer than supported" semantics of the migration
        // dispatch (rather than a blanket version mismatch).
        let tmp = tempfile::tempdir().unwrap();
        stamp_schema_version(tmp.path(), 999);

        let err = match DeletionDatabase::new(Some(tmp.path().to_path_buf())) {
            Ok(_) => panic!("newer-than-supported schema version should fail"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("newer than supported"),
            "unexpected error message: {err}"
        );
        // And it must be classified as an init failure, not a query failure.
        assert!(matches!(err, ArtifactError::DatabaseInit(_)));
    }

    #[test]
    fn opens_current_schema_version_cleanly() {
        // A database already stamped at the current version must open without
        // error and be fully usable.
        let tmp = tempfile::tempdir().unwrap();
        stamp_schema_version(tmp.path(), SCHEMA_VERSION);

        let db = DeletionDatabase::new(Some(tmp.path().to_path_buf()))
            .expect("current-version database should open cleanly");
        // Reopening (a second `new`) must also succeed and see the same data.
        db.record_deletion(&sample_record()).unwrap();
        drop(db);
        let db2 = DeletionDatabase::new(Some(tmp.path().to_path_buf()))
            .expect("reopening a current-version database should succeed");
        assert_eq!(db2.get_recent_deletions(10).unwrap().len(), 1);
    }

    #[test]
    fn backs_up_before_touching_incompatible_version() {
        // Opening a database whose version differs from the current build must
        // leave a timestamped backup copy next to the original.
        let tmp = tempfile::tempdir().unwrap();
        stamp_schema_version(tmp.path(), 999);

        // This open fails (newer than supported), but a backup must exist.
        let _ = DeletionDatabase::new(Some(tmp.path().to_path_buf()));

        let backups: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains(&format!("{DB_FILE}.bak-"))
            })
            .collect();
        assert!(
            !backups.is_empty(),
            "expected a .bak- backup of the incompatible database"
        );
    }

    #[test]
    fn aggregations_skip_corrupt_records() {
        // A single deliberately-corrupt blob in the RECORDS table must not abort
        // aggregation over the good rows.
        let (db, tmp) = temp_db();
        let good = sample_record();
        db.record_deletion(&good).unwrap();
        let good_size = good.size_bytes;

        // Inject a corrupt record blob directly into the RECORDS table under a
        // fresh id, mirroring what record_deletion writes minus a valid body.
        {
            let write_txn = db.db.begin_write().unwrap();
            {
                let mut records = write_txn.open_table(RECORDS).unwrap();
                records
                    .insert(9_999_u64, b"totally not an rkyv record".as_slice())
                    .unwrap();
            }
            write_txn.commit().unwrap();
        }

        // Both aggregations must succeed and reflect only the good row.
        let total = db.get_total_space_freed().unwrap();
        assert_eq!(total, good_size);

        let stats = db.get_deletion_statistics().unwrap();
        assert_eq!(stats.total_deletions, 1);
        assert_eq!(stats.total_space_freed, good_size);
        assert!(stats.deletions_by_type.contains_key("node_modules"));

        drop(tmp);
    }

    #[test]
    fn metadata_is_valid_json_for_all_inputs() {
        // The metadata field must be well-formed JSON carrying the schema
        // version, regardless of hostname contents (serde_json handles escaping).
        let rec = sample_record();
        let parsed: serde_json::Value = serde_json::from_str(&rec.metadata).unwrap();
        assert_eq!(parsed["version"], SCHEMA_VERSION);
    }

    #[test]
    fn corrupt_database_file_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(DB_FILE), b"not a redb database").unwrap();
        assert!(DeletionDatabase::new(Some(tmp.path().to_path_buf())).is_err());
    }
}
