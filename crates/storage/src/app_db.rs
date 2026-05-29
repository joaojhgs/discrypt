//! Versioned production application-database schema and migration planning.
//!
//! The storage crate owns the durable schema contract independently from UI
//! fixtures. Concrete shell adapters can execute the emitted SQL against their
//! SQLite handle while tests can validate migration coverage and corruption
//! quarantine behavior without depending on a Tauri process.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Current application database schema version.
pub const APP_DB_SCHEMA_VERSION: u32 = 1;

/// The first schema version supported by this crate.
pub const MIN_SUPPORTED_APP_DB_SCHEMA_VERSION: u32 = 0;

/// Durable database tables required by schema version 1.
pub const REQUIRED_TABLES: &[&str] = &[
    "profiles",
    "devices",
    "groups",
    "channels",
    "invites",
    "governance_events",
    "message_envelopes",
    "retention_state",
    "delivery_queue",
    "voice_preferences",
    "event_cursors",
];

const CREATE_PROFILES: &str = "CREATE TABLE IF NOT EXISTS profiles (profile_id TEXT PRIMARY KEY NOT NULL, user_id TEXT NOT NULL UNIQUE, display_name TEXT NOT NULL, friend_code TEXT NOT NULL, safety_number TEXT NOT NULL, safety_verified INTEGER NOT NULL DEFAULT 0, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL)";
const CREATE_DEVICES: &str = "CREATE TABLE IF NOT EXISTS devices (device_id TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, mls_leaf INTEGER NOT NULL, credential_hash BLOB NOT NULL, identity_key_ref TEXT NOT NULL, status TEXT NOT NULL, added_at_ms INTEGER NOT NULL, removed_at_ms INTEGER)";
const CREATE_GROUPS: &str = "CREATE TABLE IF NOT EXISTS groups (group_id TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, name TEXT NOT NULL, role TEXT NOT NULL, mls_epoch INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL)";
const CREATE_CHANNELS: &str = "CREATE TABLE IF NOT EXISTS channels (channel_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, name TEXT NOT NULL, kind TEXT NOT NULL, retention_preset TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL)";
const CREATE_INVITES: &str = "CREATE TABLE IF NOT EXISTS invites (invite_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, expires_at_ms INTEGER NOT NULL, max_uses INTEGER NOT NULL, password_gate TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0, created_at_ms INTEGER NOT NULL)";
const CREATE_GOVERNANCE_EVENTS: &str = "CREATE TABLE IF NOT EXISTS governance_events (event_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, sequence INTEGER NOT NULL, event_type TEXT NOT NULL, signed_payload BLOB NOT NULL, author_device_id TEXT NOT NULL, observed_at_ms INTEGER NOT NULL, UNIQUE(group_id, sequence))";
const CREATE_MESSAGE_ENVELOPES: &str = "CREATE TABLE IF NOT EXISTS message_envelopes (message_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, channel_id TEXT NOT NULL REFERENCES channels(channel_id) ON DELETE CASCADE, author_device_id TEXT NOT NULL, author_sequence INTEGER NOT NULL, mls_epoch INTEGER NOT NULL, ciphertext BLOB NOT NULL, envelope_hash BLOB NOT NULL, sent_at_ms INTEGER NOT NULL, received_at_ms INTEGER, UNIQUE(group_id, author_device_id, author_sequence))";
const CREATE_RETENTION_STATE: &str = "CREATE TABLE IF NOT EXISTS retention_state (retention_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, channel_id TEXT REFERENCES channels(channel_id) ON DELETE CASCADE, message_id TEXT REFERENCES message_envelopes(message_id) ON DELETE CASCADE, state TEXT NOT NULL, key_ref TEXT, shred_after_ms INTEGER, updated_at_ms INTEGER NOT NULL)";
const CREATE_DELIVERY_QUEUE: &str = "CREATE TABLE IF NOT EXISTS delivery_queue (queue_id TEXT PRIMARY KEY NOT NULL, message_id TEXT NOT NULL REFERENCES message_envelopes(message_id) ON DELETE CASCADE, destination TEXT NOT NULL, status TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, next_attempt_ms INTEGER NOT NULL, last_error TEXT, updated_at_ms INTEGER NOT NULL)";
const CREATE_VOICE_PREFERENCES: &str = "CREATE TABLE IF NOT EXISTS voice_preferences (profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, channel_id TEXT NOT NULL REFERENCES channels(channel_id) ON DELETE CASCADE, muted INTEGER NOT NULL DEFAULT 0, speaker_volume INTEGER NOT NULL DEFAULT 100, route TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(profile_id, group_id, channel_id))";
const CREATE_EVENT_CURSORS: &str = "CREATE TABLE IF NOT EXISTS event_cursors (cursor_id TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, source TEXT NOT NULL, position TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, UNIQUE(profile_id, source))";

const CREATE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_devices_profile ON devices(profile_id)",
    "CREATE INDEX IF NOT EXISTS idx_channels_group ON channels(group_id)",
    "CREATE INDEX IF NOT EXISTS idx_messages_channel_time ON message_envelopes(channel_id, sent_at_ms)",
    "CREATE INDEX IF NOT EXISTS idx_delivery_status_attempt ON delivery_queue(status, next_attempt_ms)",
    "CREATE INDEX IF NOT EXISTS idx_retention_message ON retention_state(message_id)",
    "CREATE INDEX IF NOT EXISTS idx_governance_group_sequence ON governance_events(group_id, sequence)",
];

const VERSION_1_DDL: &[&str] = &[
    "PRAGMA foreign_keys = ON",
    CREATE_PROFILES,
    CREATE_DEVICES,
    CREATE_GROUPS,
    CREATE_CHANNELS,
    CREATE_INVITES,
    CREATE_GOVERNANCE_EVENTS,
    CREATE_MESSAGE_ENVELOPES,
    CREATE_RETENTION_STATE,
    CREATE_DELIVERY_QUEUE,
    CREATE_VOICE_PREFERENCES,
    CREATE_EVENT_CURSORS,
    CREATE_INDEXES[0],
    CREATE_INDEXES[1],
    CREATE_INDEXES[2],
    CREATE_INDEXES[3],
    CREATE_INDEXES[4],
    CREATE_INDEXES[5],
    "PRAGMA user_version = 1",
];

const VERSION_1_ROLLBACK: &[&str] = &[
    "DROP TABLE IF EXISTS event_cursors",
    "DROP TABLE IF EXISTS voice_preferences",
    "DROP TABLE IF EXISTS delivery_queue",
    "DROP TABLE IF EXISTS retention_state",
    "DROP TABLE IF EXISTS message_envelopes",
    "DROP TABLE IF EXISTS governance_events",
    "DROP TABLE IF EXISTS invites",
    "DROP TABLE IF EXISTS channels",
    "DROP TABLE IF EXISTS groups",
    "DROP TABLE IF EXISTS devices",
    "DROP TABLE IF EXISTS profiles",
    "PRAGMA user_version = 0",
];

/// A required column in a schema table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDbColumn {
    /// Column name.
    pub name: &'static str,
    /// Stable SQL type or affinity used by the migration contract.
    pub sql_type: &'static str,
    /// Whether the field may contain secret material and must be keychain-wrapped/encrypted by writers.
    pub sensitive: bool,
}

/// A required table in a schema version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDbTable {
    /// Table name.
    pub name: &'static str,
    /// Required columns for corruption/migration verification.
    pub columns: &'static [AppDbColumn],
}

/// Durable schema manifest for the current application database.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDbSchema {
    /// Schema version represented by this manifest.
    pub version: u32,
    /// Required tables.
    pub tables: &'static [AppDbTable],
}

/// Direction for a schema migration plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationDirection {
    /// Upgrade from an older supported schema.
    Forward,
    /// Roll back to an older supported schema for tests/recovery validation.
    Backward,
    /// No schema changes are required.
    Noop,
}

/// SQL migration plan between two supported versions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDbMigrationPlan {
    /// Source schema version.
    pub from_version: u32,
    /// Target schema version.
    pub to_version: u32,
    /// Migration direction.
    pub direction: MigrationDirection,
    /// Ordered SQL statements to execute transactionally.
    pub statements: Vec<&'static str>,
}

/// Quarantine result for a corrupted database and sidecar files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuarantinedAppDb {
    /// Original database path.
    pub original_path: PathBuf,
    /// Quarantined database path.
    pub quarantine_path: PathBuf,
    /// Quarantined sidecars, such as WAL and SHM files.
    pub sidecars: Vec<(PathBuf, PathBuf)>,
}

/// Application database schema and migration errors.
#[derive(Debug, thiserror::Error)]
pub enum AppDbError {
    /// A requested migration version is newer than this crate understands.
    #[error("unsupported future app DB schema version {version}; current is {current}")]
    UnsupportedFutureVersion { version: u32, current: u32 },
    /// A requested migration version is older than the supported floor.
    #[error("unsupported legacy app DB schema version {version}; minimum is {minimum}")]
    UnsupportedLegacyVersion { version: u32, minimum: u32 },
    /// The observed store is missing a required table.
    #[error("corrupt app DB: missing required table {table}")]
    MissingRequiredTable { table: &'static str },
    /// The observed store is missing a required column.
    #[error("corrupt app DB: missing required column {table}.{column}")]
    MissingRequiredColumn {
        table: &'static str,
        column: &'static str,
    },
    /// Corruption quarantine failed at the filesystem boundary.
    #[error("app DB quarantine I/O error: {0}")]
    QuarantineIo(#[from] std::io::Error),
}

const PROFILE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "user_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "display_name",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "friend_code",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "safety_number",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "safety_verified",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const DEVICE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "device_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "mls_leaf",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "credential_hash",
        sql_type: "BLOB",
        sensitive: false,
    },
    AppDbColumn {
        name: "identity_key_ref",
        sql_type: "TEXT",
        sensitive: true,
    },
    AppDbColumn {
        name: "status",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "added_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "removed_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const GROUP_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "name",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "role",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "mls_epoch",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const CHANNEL_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "name",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "kind",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "retention_preset",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const INVITE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "invite_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "expires_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "max_uses",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "password_gate",
        sql_type: "TEXT",
        sensitive: true,
    },
    AppDbColumn {
        name: "revoked",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const GOVERNANCE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "event_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "sequence",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "event_type",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "signed_payload",
        sql_type: "BLOB",
        sensitive: false,
    },
    AppDbColumn {
        name: "author_device_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "observed_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const MESSAGE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "message_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "author_device_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "author_sequence",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "mls_epoch",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "ciphertext",
        sql_type: "BLOB",
        sensitive: true,
    },
    AppDbColumn {
        name: "envelope_hash",
        sql_type: "BLOB",
        sensitive: false,
    },
    AppDbColumn {
        name: "sent_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "received_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const RETENTION_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "retention_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "message_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "state",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "key_ref",
        sql_type: "TEXT",
        sensitive: true,
    },
    AppDbColumn {
        name: "shred_after_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const DELIVERY_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "queue_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "message_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "destination",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "status",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "attempts",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "next_attempt_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "last_error",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const VOICE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "muted",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "speaker_volume",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "route",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const CURSOR_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "cursor_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "source",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "position",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const APP_DB_TABLES: &[AppDbTable] = &[
    AppDbTable {
        name: "profiles",
        columns: PROFILE_COLUMNS,
    },
    AppDbTable {
        name: "devices",
        columns: DEVICE_COLUMNS,
    },
    AppDbTable {
        name: "groups",
        columns: GROUP_COLUMNS,
    },
    AppDbTable {
        name: "channels",
        columns: CHANNEL_COLUMNS,
    },
    AppDbTable {
        name: "invites",
        columns: INVITE_COLUMNS,
    },
    AppDbTable {
        name: "governance_events",
        columns: GOVERNANCE_COLUMNS,
    },
    AppDbTable {
        name: "message_envelopes",
        columns: MESSAGE_COLUMNS,
    },
    AppDbTable {
        name: "retention_state",
        columns: RETENTION_COLUMNS,
    },
    AppDbTable {
        name: "delivery_queue",
        columns: DELIVERY_COLUMNS,
    },
    AppDbTable {
        name: "voice_preferences",
        columns: VOICE_COLUMNS,
    },
    AppDbTable {
        name: "event_cursors",
        columns: CURSOR_COLUMNS,
    },
];

impl AppDbSchema {
    /// Return the current durable schema manifest.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            version: APP_DB_SCHEMA_VERSION,
            tables: APP_DB_TABLES,
        }
    }

    /// Find a required table by name.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&'static AppDbTable> {
        self.tables.iter().find(|table| table.name == name)
    }

    /// Iterate all columns that carry sensitive material or key references.
    pub fn sensitive_columns(&self) -> impl Iterator<Item = (&'static str, &'static AppDbColumn)> {
        self.tables.iter().flat_map(|table| {
            table
                .columns
                .iter()
                .filter(|column| column.sensitive)
                .map(move |column| (table.name, column))
        })
    }
}

impl AppDbMigrationPlan {
    /// Build a supported migration plan between schema versions.
    pub fn plan(from_version: u32, to_version: u32) -> Result<Self, AppDbError> {
        validate_version(from_version)?;
        validate_version(to_version)?;

        if from_version == to_version {
            return Ok(Self {
                from_version,
                to_version,
                direction: MigrationDirection::Noop,
                statements: Vec::new(),
            });
        }

        if from_version == 0 && to_version == 1 {
            return Ok(Self {
                from_version,
                to_version,
                direction: MigrationDirection::Forward,
                statements: VERSION_1_DDL.to_vec(),
            });
        }

        if from_version == 1 && to_version == 0 {
            return Ok(Self {
                from_version,
                to_version,
                direction: MigrationDirection::Backward,
                statements: VERSION_1_ROLLBACK.to_vec(),
            });
        }

        // The version validator keeps this arm unreachable for the current two-version graph,
        // but keeping the explicit future error makes added versions fail safely.
        Err(AppDbError::UnsupportedFutureVersion {
            version: to_version,
            current: APP_DB_SCHEMA_VERSION,
        })
    }

    /// True when the plan has statements to execute.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}

/// Validate an observed schema inventory against the current manifest.
pub fn validate_observed_schema<'a, I, J>(tables: I) -> Result<(), AppDbError>
where
    I: IntoIterator<Item = (&'a str, J)>,
    J: IntoIterator<Item = &'a str>,
{
    let observed: Vec<(&str, Vec<&str>)> = tables
        .into_iter()
        .map(|(table, columns)| (table, columns.into_iter().collect()))
        .collect();

    for required in AppDbSchema::current().tables {
        let Some((_, columns)) = observed.iter().find(|(table, _)| *table == required.name) else {
            return Err(AppDbError::MissingRequiredTable {
                table: required.name,
            });
        };
        for column in required.columns {
            if !columns
                .iter()
                .any(|observed_column| *observed_column == column.name)
            {
                return Err(AppDbError::MissingRequiredColumn {
                    table: required.name,
                    column: column.name,
                });
            }
        }
    }
    Ok(())
}

/// Move a corrupt database and its WAL/SHM sidecars aside before opening a fresh store.
pub fn quarantine_corrupt_store(path: impl AsRef<Path>) -> Result<QuarantinedAppDb, AppDbError> {
    let original_path = path.as_ref().to_path_buf();
    let quarantine_path = corruption_path(&original_path, "db");
    fs::rename(&original_path, &quarantine_path)?;

    let mut sidecars = Vec::new();
    for suffix in ["wal", "shm", "journal"] {
        let sidecar = sidecar_path(&original_path, suffix);
        if sidecar.exists() {
            let quarantined = corruption_path(&sidecar, suffix);
            fs::rename(&sidecar, &quarantined)?;
            sidecars.push((sidecar, quarantined));
        }
    }

    Ok(QuarantinedAppDb {
        original_path,
        quarantine_path,
        sidecars,
    })
}

fn validate_version(version: u32) -> Result<(), AppDbError> {
    if version > APP_DB_SCHEMA_VERSION {
        return Err(AppDbError::UnsupportedFutureVersion {
            version,
            current: APP_DB_SCHEMA_VERSION,
        });
    }
    if version < MIN_SUPPORTED_APP_DB_SCHEMA_VERSION {
        return Err(AppDbError::UnsupportedLegacyVersion {
            version,
            minimum: MIN_SUPPORTED_APP_DB_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(format!("-{suffix}"));
    PathBuf::from(raw)
}

fn corruption_path(path: &Path, tag: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let mut raw = path.as_os_str().to_os_string();
    raw.push(format!(".corrupt-{tag}-{timestamp}"));
    PathBuf::from(raw)
}

impl fmt::Display for AppDbMigrationPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "app DB migration {} -> {} ({:?}, {} statements)",
            self.from_version,
            self.to_version,
            self.direction,
            self.statements.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::io::Write;

    #[test]
    fn current_schema_has_all_phase_b_tables() {
        let schema = AppDbSchema::current();
        assert_eq!(schema.version, APP_DB_SCHEMA_VERSION);
        let table_names = schema
            .tables
            .iter()
            .map(|table| table.name)
            .collect::<BTreeSet<_>>();

        for required in REQUIRED_TABLES {
            assert!(table_names.contains(required), "missing {required}");
        }
    }

    #[test]
    fn migration_from_empty_store_creates_required_schema() -> Result<(), AppDbError> {
        let plan = AppDbMigrationPlan::plan(0, APP_DB_SCHEMA_VERSION)?;
        assert_eq!(plan.direction, MigrationDirection::Forward);
        assert!(!plan.is_empty());
        for required in REQUIRED_TABLES {
            let needle = format!("CREATE TABLE IF NOT EXISTS {required}");
            assert!(
                plan.statements
                    .iter()
                    .any(|statement| statement.contains(&needle)),
                "missing migration statement for {required}"
            );
        }
        assert!(plan
            .statements
            .iter()
            .any(|statement| *statement == "PRAGMA user_version = 1"));
        Ok(())
    }

    #[test]
    fn backward_migration_drops_required_schema_for_recovery_tests() -> Result<(), AppDbError> {
        let plan = AppDbMigrationPlan::plan(APP_DB_SCHEMA_VERSION, 0)?;
        assert_eq!(plan.direction, MigrationDirection::Backward);
        for required in REQUIRED_TABLES {
            let needle = format!("DROP TABLE IF EXISTS {required}");
            assert!(
                plan.statements
                    .iter()
                    .any(|statement| statement.contains(&needle)),
                "missing rollback statement for {required}"
            );
        }
        Ok(())
    }

    #[test]
    fn schema_validation_reports_missing_table_and_column() {
        let missing_table = validate_observed_schema([(
            "profiles",
            PROFILE_COLUMNS.iter().map(|column| column.name),
        )]);
        assert!(matches!(
            missing_table,
            Err(AppDbError::MissingRequiredTable { table: "devices" })
        ));

        let observed = AppDbSchema::current().tables.iter().map(|table| {
            let columns = table
                .columns
                .iter()
                .filter(|column| !(table.name == "devices" && column.name == "identity_key_ref"))
                .map(|column| column.name);
            (table.name, columns)
        });
        let missing_column = validate_observed_schema(observed);
        assert!(matches!(
            missing_column,
            Err(AppDbError::MissingRequiredColumn {
                table: "devices",
                column: "identity_key_ref",
            })
        ));
    }

    #[test]
    fn sensitive_fields_are_key_references_or_ciphertext_only() {
        let sensitive = AppDbSchema::current()
            .sensitive_columns()
            .map(|(table, column)| format!("{table}.{}", column.name))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            sensitive,
            BTreeSet::from([
                "devices.identity_key_ref".to_owned(),
                "invites.password_gate".to_owned(),
                "message_envelopes.ciphertext".to_owned(),
                "retention_state.key_ref".to_owned(),
            ])
        );
    }

    #[test]
    fn corrupt_store_quarantine_moves_db_and_sidecars() -> Result<(), Box<dyn std::error::Error>> {
        let base = std::env::temp_dir().join(format!(
            "discrypt-app-db-corrupt-{}-{}.sqlite",
            std::process::id(),
            unique_test_suffix()
        ));
        let wal = sidecar_path(&base, "wal");
        let shm = sidecar_path(&base, "shm");

        write_file(&base, b"not sqlite")?;
        write_file(&wal, b"wal bytes")?;
        write_file(&shm, b"shm bytes")?;

        let quarantined = quarantine_corrupt_store(&base)?;
        assert!(!base.exists());
        assert!(!wal.exists());
        assert!(!shm.exists());
        assert!(quarantined.quarantine_path.exists());
        assert_eq!(quarantined.sidecars.len(), 2);
        for (_, quarantined_sidecar) in &quarantined.sidecars {
            assert!(quarantined_sidecar.exists());
            let _ = fs::remove_file(quarantined_sidecar);
        }
        let _ = fs::remove_file(quarantined.quarantine_path);
        Ok(())
    }

    fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(bytes)
    }

    fn unique_test_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    }
}
