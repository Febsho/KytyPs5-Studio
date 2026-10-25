use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::{fd::AsRawFd, unix::net::UnixStream, unix::process::CommandExt};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
#[cfg(windows)]
trait ProcessGroupCommandExt {
    fn process_group(&mut self, group_id: i32) -> &mut Command;
}
#[cfg(windows)]
impl ProcessGroupCommandExt for Command {
    fn process_group(&mut self, _group_id: i32) -> &mut Command {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_SUSPENDED: u32 = 0x0000_0004;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        self.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED | CREATE_NO_WINDOW)
    }
}
#[path = "save_data.rs"]
mod save_data;
#[path = "steam.rs"]
mod steam;
#[path = "steam_shortcuts.rs"]
mod steam_shortcuts;
#[path = "trophies.rs"]
mod trophies;

#[cfg(any(windows, test))]
#[cfg_attr(test, allow(dead_code))]
#[path = "platform/windows_build.rs"]
mod windows_build;
#[cfg(windows)]
#[path = "platform/windows_credentials.rs"]
mod windows_credentials;
#[cfg(windows)]
#[path = "platform/windows_diagnostics.rs"]
mod windows_diagnostics;
#[cfg(windows)]
#[path = "platform/windows_process.rs"]
mod windows_process;

#[cfg(test)]
const OFFICIAL: &str = "https://github.com/KytyPS5/KytyPS5";
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn absolute_env_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}
#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}
#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // Keep replacement atomic and ask Windows to flush the move before returning.
    const MOVEFILE_REPLACE_EXISTING_AND_WRITE_THROUGH: u32 = 0x1 | 0x8;
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING_AND_WRITE_THROUGH,
        )
    } != 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
fn path_starts_with(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Globalization::CompareStringOrdinal;

        fn separator(unit: u16) -> bool {
            unit == b'\\' as u16 || unit == b'/' as u16
        }

        let path = path.as_os_str().encode_wide().collect::<Vec<_>>();
        let root = root.as_os_str().encode_wide().collect::<Vec<_>>();
        let mut root_len = root.len();
        while root_len > 1 && separator(root[root_len - 1]) {
            root_len -= 1;
        }
        if root_len == 0 || path.len() < root_len {
            return false;
        }
        let Ok(root_len_i32) = i32::try_from(root_len) else {
            return false;
        };
        if unsafe {
            CompareStringOrdinal(path.as_ptr(), root_len_i32, root.as_ptr(), root_len_i32, 1)
        } != 2
        {
            return false;
        }
        path.len() == root_len || separator(path[root_len])
    }
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}
fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        path_starts_with(left, right) && path_starts_with(right, left)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
fn emulator_binary_name() -> &'static str {
    if cfg!(windows) {
        "kyty_emulator.exe"
    } else {
        "kyty_emulator"
    }
}
fn emulator_executable(directory: &Path) -> PathBuf {
    directory.join(emulator_binary_name())
}
fn is_emulator_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    }
}
#[cfg(unix)]
fn home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is unavailable".into())
}
#[cfg(windows)]
fn home() -> Result<PathBuf, String> {
    env::var_os("USERPROFILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            Some(PathBuf::from(drive).join(path))
        })
        .ok_or_else(|| "USERPROFILE is unavailable".to_string())
}
#[derive(Clone)]
struct Paths {
    data: PathBuf,
    cache: PathBuf,
    config: PathBuf,
}
impl Paths {
    fn new() -> Result<Self, String> {
        let h = home()?;
        #[cfg(unix)]
        let path = |key: &str, fallback: &str| {
            absolute_env_path(env::var_os(key))
                .unwrap_or_else(|| h.join(fallback))
                .join("kytyps5-studio")
        };
        #[cfg(unix)]
        let p = Self {
            data: path("XDG_DATA_HOME", ".local/share"),
            cache: path("XDG_CACHE_HOME", ".cache"),
            config: path("XDG_CONFIG_HOME", ".config"),
        };
        #[cfg(windows)]
        let p = Self::from_windows_roots(
            absolute_env_path(env::var_os("LOCALAPPDATA"))
                .unwrap_or_else(|| h.join("AppData").join("Local")),
            absolute_env_path(env::var_os("APPDATA"))
                .unwrap_or_else(|| h.join("AppData").join("Roaming")),
        );
        for d in [&p.data, &p.cache, &p.config] {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        Ok(p)
    }
    #[cfg(windows)]
    fn from_windows_roots(local: PathBuf, roaming: PathBuf) -> Self {
        Self {
            data: local.join("kytyps5-studio"),
            cache: local.join("kytyps5-studio").join("Cache"),
            config: roaming.join("kytyps5-studio"),
        }
    }
    fn db(&self) -> Result<Connection, String> {
        let mut db =
            Connection::open(self.data.join("studio.sqlite3")).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .map_err(|e| e.to_string())?;
        migrate(&mut db)?;
        Ok(db)
    }
}
fn migrate(db: &mut Connection) -> Result<(), String> {
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 1 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS repositories(id INTEGER PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, url TEXT NOT NULL UNIQUE, branch TEXT NOT NULL, source_path TEXT, adapter TEXT NOT NULL DEFAULT 'unknown', installed_commit TEXT);
            CREATE TABLE IF NOT EXISTS builds(id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repositories(id), commit_sha TEXT NOT NULL, build_type TEXT NOT NULL, path TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL, active INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS games(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL, favorite INTEGER NOT NULL DEFAULT 0, build_id INTEGER REFERENCES builds(id), play_seconds INTEGER NOT NULL DEFAULT 0, last_played INTEGER);
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions(id INTEGER PRIMARY KEY, game_id INTEGER NOT NULL REFERENCES games(id), build_id INTEGER NOT NULL REFERENCES builds(id), started_at INTEGER NOT NULL, ended_at INTEGER, exit_code INTEGER, outcome TEXT, log_path TEXT NOT NULL);")
            .map_err(|e|e.to_string())?;
        tx.pragma_update(None, "user_version", 1)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 2 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS profiles(game_id INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE, screen_width INTEGER, screen_height INTEGER, present_mode TEXT, perf_overlay INTEGER NOT NULL DEFAULT 0);")
            .map_err(|e|e.to_string())?;
        tx.pragma_update(None, "user_version", 2)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 3 {
        let mut columns = Vec::new();
        let mut statement = db
            .prepare("PRAGMA table_info(games)")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?;
        for column in rows {
            columns.push(column.map_err(|e| e.to_string())?);
        }
        drop(statement);
        let tx = db.transaction().map_err(|e| e.to_string())?;
        for (name, definition) in [
            ("title_id", "TEXT"),
            ("game_version", "TEXT"),
            ("icon_path", "TEXT"),
            ("install_size", "INTEGER"),
        ] {
            if !columns.iter().any(|column| column == name) {
                tx.execute_batch(&format!("ALTER TABLE games ADD COLUMN {name} {definition}"))
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.pragma_update(None, "user_version", 3)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 4 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch(
            "ALTER TABLE sessions ADD COLUMN process_pid INTEGER;
             ALTER TABLE sessions ADD COLUMN process_start_ticks INTEGER;",
        )
        .map_err(|e| e.to_string())?;
        tx.pragma_update(None, "user_version", 4)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 5 {
        let mut statement = db
            .prepare("PRAGMA table_info(profiles)")
            .map_err(|e| e.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(statement);
        let tx = db.transaction().map_err(|e| e.to_string())?;
        if !columns.iter().any(|column| column == "fullscreen") {
            tx.execute_batch(
                "ALTER TABLE profiles ADD COLUMN fullscreen INTEGER NOT NULL DEFAULT 0;",
            )
            .map_err(|e| e.to_string())?;
        }
        tx.pragma_update(None, "user_version", 5)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 6 {
        let mut statement = db
            .prepare("PRAGMA table_info(profiles)")
            .map_err(|e| e.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(statement);
        let tx = db.transaction().map_err(|e| e.to_string())?;
        for (name, definition) in [
            ("controller_color", "TEXT"),
            ("controller_volume", "INTEGER"),
            ("controller_vibration", "INTEGER"),
            ("keyboard_player", "INTEGER"),
        ] {
            if !columns.iter().any(|column| column == name) {
                tx.execute_batch(&format!(
                    "ALTER TABLE profiles ADD COLUMN {name} {definition}"
                ))
                .map_err(|e| e.to_string())?;
            }
        }
        tx.pragma_update(None, "user_version", 6)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 7 {
        let mut statement = db
            .prepare("PRAGMA table_info(builds)")
            .map_err(|e| e.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(statement);
        let tx = db.transaction().map_err(|e| e.to_string())?;
        for (name, definition) in [
            ("install_size_bytes", "INTEGER"),
            ("validation_status", "TEXT NOT NULL DEFAULT 'not-checked'"),
        ] {
            if !columns.iter().any(|column| column == name) {
                tx.execute_batch(&format!(
                    "ALTER TABLE builds ADD COLUMN {name} {definition}"
                ))
                .map_err(|e| e.to_string())?;
            }
        }
        tx.pragma_update(None, "user_version", 7)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 8 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS community_pr_cache(
                repo_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                number INTEGER NOT NULL,
                payload TEXT NOT NULL,
                cached_at INTEGER NOT NULL,
                PRIMARY KEY(repo_id, number)
             );
             CREATE TABLE IF NOT EXISTS community_build_configs(
                id INTEGER PRIMARY KEY,
                signature TEXT NOT NULL UNIQUE,
                repo_id INTEGER NOT NULL REFERENCES repositories(id),
                name TEXT NOT NULL,
                base_branch TEXT NOT NULL,
                base_sha TEXT NOT NULL,
                build_type TEXT NOT NULL,
                jobs INTEGER NOT NULL,
                created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS community_build_prs(
                config_id INTEGER NOT NULL REFERENCES community_build_configs(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                number INTEGER NOT NULL,
                head_repo TEXT NOT NULL,
                head_sha TEXT NOT NULL,
                PRIMARY KEY(config_id, position),
                UNIQUE(config_id, number)
             );
             CREATE TABLE IF NOT EXISTS community_build_results(
                id INTEGER PRIMARY KEY,
                config_id INTEGER NOT NULL REFERENCES community_build_configs(id),
                build_id INTEGER REFERENCES builds(id),
                final_commit TEXT,
                install_path TEXT,
                status TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                log_path TEXT,
                error TEXT
             );",
        )
        .map_err(|e| e.to_string())?;
        tx.pragma_update(None, "user_version", 8)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 9 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        for (name, definition) in [
            ("compiler", "TEXT NOT NULL DEFAULT 'clang'"),
            ("clean", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('community_build_configs') WHERE name=?)", [name], |row| row.get(0)).map_err(|e| e.to_string())?;
            if !exists {
                tx.execute_batch(&format!(
                    "ALTER TABLE community_build_configs ADD COLUMN {name} {definition}"
                ))
                .map_err(|e| e.to_string())?;
            }
        }
        tx.pragma_update(None, "user_version", 9)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 10 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        // GitHub credentials belong in Secret Service. Remove only the
        // insecure legacy generic-setting copy; existing Secret Service
        // credentials and all other settings remain intact.
        let settings_table_exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if settings_table_exists {
            tx.execute("DELETE FROM settings WHERE key='github_token'", [])
                .map_err(|e| e.to_string())?;
        }
        tx.pragma_update(None, "user_version", 10)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 11 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let table_exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='profiles')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let exists: bool = table_exists && tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('profiles') WHERE name='options_json')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if table_exists && !exists {
            tx.execute_batch(
                "ALTER TABLE profiles ADD COLUMN options_json TEXT NOT NULL DEFAULT '{}';",
            )
            .map_err(|e| e.to_string())?;
        }
        tx.pragma_update(None, "user_version", 11)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 12 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS global_profile_defaults(id INTEGER PRIMARY KEY CHECK(id=1), options_json TEXT NOT NULL DEFAULT '{}'); INSERT OR IGNORE INTO global_profile_defaults(id,options_json) VALUES(1,'{}');")
            .map_err(|e| e.to_string())?;
        tx.pragma_update(None, "user_version", 12)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 13 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let has_games: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='games')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if has_games {
            let has_hidden: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('games') WHERE name='hidden')",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !has_hidden {
                tx.execute_batch("ALTER TABLE games ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;")
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.pragma_update(None, "user_version", 13)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 14 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS manual_compatibility_observations(id INTEGER PRIMARY KEY,game_id INTEGER NOT NULL REFERENCES games(id),repo_id INTEGER NOT NULL REFERENCES repositories(id),build_id INTEGER NOT NULL REFERENCES builds(id),commit_sha TEXT NOT NULL,build_type TEXT NOT NULL,patch_config_json TEXT NOT NULL, outcome TEXT NOT NULL,notes TEXT NOT NULL DEFAULT '',observed_at INTEGER NOT NULL);")
            .map_err(|e| e.to_string())?;
        tx.pragma_update(None, "user_version", 14)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 15 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let columns = {
            let mut statement = tx
                .prepare("PRAGMA table_info(builds)")
                .map_err(|e| e.to_string())?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        if !columns.is_empty() && !columns.iter().any(|column| column == "configuration_json") {
            tx.execute_batch(
                "ALTER TABLE builds ADD COLUMN configuration_json TEXT NOT NULL DEFAULT '{}';",
            )
            .map_err(|e| e.to_string())?;
        }
        tx.pragma_update(None, "user_version", 15)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 16 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let columns = {
            let mut statement = tx
                .prepare("PRAGMA table_info(builds)")
                .map_err(|e| e.to_string())?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        if !columns.is_empty() && !columns.iter().any(|column| column == "removed_at") {
            tx.execute_batch("ALTER TABLE builds ADD COLUMN removed_at INTEGER;")
                .map_err(|e| e.to_string())?;
        }
        tx.pragma_update(None, "user_version", 16)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 17 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let columns = {
            let mut statement = tx
                .prepare("PRAGMA table_info(repositories)")
                .map_err(|e| e.to_string())?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        if !columns.is_empty() && !columns.iter().any(|column| column == "removed_at") {
            tx.execute_batch("ALTER TABLE repositories ADD COLUMN removed_at INTEGER;")
                .map_err(|e| e.to_string())?;
        }
        tx.pragma_update(None, "user_version", 17)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 18 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        // Install size is derived data; invalidate older values so the new
        // package-root calculation refreshes existing library entries.
        let has_install_size: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('games') WHERE name='install_size')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if has_install_size {
            tx.execute("UPDATE games SET install_size=NULL", [])
                .map_err(|e| e.to_string())?;
        }
        tx.pragma_update(None, "user_version", 18)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 19 {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS build_profile_defaults(build_id INTEGER PRIMARY KEY REFERENCES builds(id) ON DELETE CASCADE, options_json TEXT NOT NULL DEFAULT '{}');
             INSERT OR IGNORE INTO build_profile_defaults(build_id, options_json)
             SELECT id, (SELECT options_json FROM global_profile_defaults WHERE id=1)
             FROM builds WHERE active=1 AND EXISTS(SELECT 1 FROM global_profile_defaults WHERE id=1);",
        )
        .map_err(|e| e.to_string())?;
        tx.pragma_update(None, "user_version", 19)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[derive(Serialize)]
struct Repo {
    id: i64,
    owner: String,
    name: String,
    url: String,
    branch: String,
    source_path: Option<String>,
    adapter: String,
    installed_commit: Option<String>,
    removed: bool,
}
#[derive(Serialize)]
struct Build {
    id: i64,
    repo_id: i64,
    commit_sha: String,
    build_type: String,
    path: String,
    created_at: i64,
    active: bool,
    executable_status: String,
    install_size_bytes: Option<u64>,
    validation_status: String,
    configuration: BuildConfiguration,
    last_session_at: Option<i64>,
    last_session_outcome: Option<String>,
    last_session_exit_code: Option<i32>,
    community_prs: Vec<BuildPullRequest>,
}
#[derive(Serialize)]
struct BuildPullRequest {
    number: u64,
    head_sha: String,
    url: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct BuildConfiguration {
    compiler: Option<String>,
    jobs: Option<u16>,
    clean: Option<bool>,
    cmake_arguments: Vec<String>,
}
#[derive(Serialize)]
struct Game {
    id: i64,
    path: String,
    title: String,
    favorite: bool,
    build_id: Option<i64>,
    play_seconds: i64,
    last_played: Option<i64>,
    title_id: Option<String>,
    game_version: Option<String>,
    icon_path: Option<String>,
    install_size: Option<i64>,
    file_status: String,
}
#[derive(Serialize)]
struct ShaderPrecompileSupport {
    full_precompile: bool,
    background_precompile: bool,
    background_enabled: bool,
    cache_directory: String,
}
#[derive(Serialize)]
struct GameCompatibility {
    path_recognized: bool,
    launch_attempts: i64,
    emulator_process_starts: i64,
    last_outcome: Option<String>,
    last_exit_code: Option<i32>,
    emulator_reported_title_id: bool,
    latest_log_path: Option<String>,
    gameplay_status: String,
    playable_status: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct OfficialCompatibilityGame {
    title_id: String,
    title: String,
    overall: String,
    status: String,
    preferred_os: String,
    os: HashMap<String, String>,
    report_counts: HashMap<String, u32>,
    latest_tested: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct OfficialCompatibilityIndex {
    available: bool,
    updated_at: Option<i64>,
    error: Option<String>,
    games: Vec<OfficialCompatibilityGame>,
}
#[derive(Deserialize)]
struct OfficialSiteIndexPayload {
    games: Vec<OfficialSiteIndexGame>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialSiteIndexGame {
    key: Option<String>,
    title_id: Option<String>,
    title: String,
    overall: String,
    os: HashMap<String, String>,
    report_counts: HashMap<String, u32>,
    latest_tested: Option<String>,
}
#[derive(Serialize)]
struct ManualCompatibilityObservation {
    id: i64,
    outcome: String,
    notes: String,
    observed_at: i64,
    repository: String,
    build_id: i64,
    build_commit: String,
    build_type: String,
    patch_config_json: String,
}
#[derive(Deserialize, Serialize, Default)]
struct Profile {
    screen_width: Option<u32>,
    screen_height: Option<u32>,
    present_mode: Option<String>,
    perf_overlay: bool,
    fullscreen: bool,
    controller_color: Option<String>,
    controller_volume: Option<u32>,
    controller_vibration: Option<u32>,
    keyboard_player: Option<u32>,
    #[serde(default)]
    options: BTreeMap<String, String>,
}

fn merge_legacy_profile_options(profile: &mut Profile) {
    let mut insert = |flag: &str, value: Option<String>| {
        if let Some(value) = value {
            profile.options.entry(flag.to_string()).or_insert(value);
        }
    };
    insert(
        "--screen-width",
        profile.screen_width.map(|value| value.to_string()),
    );
    insert(
        "--screen-height",
        profile.screen_height.map(|value| value.to_string()),
    );
    insert("--present-mode", profile.present_mode.clone());
    if profile.perf_overlay {
        insert("--perf-overlay", Some("true".into()));
    }
    if profile.fullscreen {
        insert("--fullscreen", Some("true".into()));
    }
    insert("--controller-color", profile.controller_color.clone());
    insert(
        "--controller-volume",
        profile.controller_volume.map(|value| value.to_string()),
    );
    insert(
        "--controller-vibration",
        profile.controller_vibration.map(|value| value.to_string()),
    );
    insert(
        "--keyboard-player",
        profile.keyboard_player.map(|value| value.to_string()),
    );
}

fn sync_legacy_profile_fields(profile: &mut Profile) {
    profile.screen_width = profile
        .options
        .get("--screen-width")
        .and_then(|v| v.parse().ok());
    profile.screen_height = profile
        .options
        .get("--screen-height")
        .and_then(|v| v.parse().ok());
    profile.present_mode = profile.options.get("--present-mode").cloned();
    profile.perf_overlay = profile
        .options
        .get("--perf-overlay")
        .is_some_and(|value| value == "true");
    profile.fullscreen = profile
        .options
        .get("--fullscreen")
        .is_some_and(|value| value == "true");
    profile.controller_color = profile.options.get("--controller-color").cloned();
    profile.controller_volume = profile
        .options
        .get("--controller-volume")
        .and_then(|v| v.parse().ok());
    profile.controller_vibration = profile
        .options
        .get("--controller-vibration")
        .and_then(|v| v.parse().ok());
    profile.keyboard_player = profile
        .options
        .get("--keyboard-player")
        .and_then(|v| v.parse().ok());
}

#[derive(Clone, Debug, Serialize)]
struct ProfileCapabilities {
    screen_width: bool,
    screen_height: bool,
    present_mode: bool,
    perf_overlay: bool,
    fullscreen: bool,
    controller_color: bool,
    controller_volume: bool,
    controller_vibration: bool,
    keyboard_player: bool,
    options: Vec<String>,
    verified_options: Vec<String>,
    option_states: HashMap<String, CapabilityStatus>,
    binary_sha256: String,
    host_os: String,
    background_precompile: bool,
    switch_options: Vec<String>,
    option_metadata: HashMap<String, CliOptionMetadata>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityState {
    VerifiedSupported,
    AdvertisedUnverified,
    Unavailable,
    Unsupported,
}

#[derive(Clone, Debug, Serialize)]
struct CapabilityStatus {
    state: CapabilityState,
    explanation: String,
}

#[derive(Clone, Debug, Serialize)]
struct CliOptionMetadata {
    kind: String,
    choices: Vec<String>,
    minimum: Option<i64>,
    maximum: Option<i64>,
    default_value: Option<String>,
    description: String,
    state: CapabilityState,
    state_explanation: String,
}

const CONFIGURABLE_CLI_OPTIONS: &[&str] = &[
    "--addcont-dir",
    "--amd-cpu",
    "--command-buffer-dump",
    "--command-buffer-dump-folder",
    "--console-language",
    "--controller-color",
    "--controller-volume",
    "--controller-vibration",
    "--fullscreen",
    "--game-patch",
    "--gpu",
    "--gpu-assisted-validation",
    "--graphics-debug-dump",
    "--hdr",
    "--hide-cursor",
    "--keyboard-player",
    "--keymap",
    "--mic",
    "--network-connected",
    "--no-splash",
    "--playgo-hack",
    "--perf-overlay",
    "--present-mode",
    "--printf-direction",
    "--printf-output-file",
    "--profile",
    "--readback-linear-images",
    "--rd",
    "--shader-log-direction",
    "--shader-log-folder",
    "--shader-optimization-type",
    "--shader-validation",
    "--screen-height",
    "--screen-width",
    "--skip-notice-screen",
    "--spirv-debug-printf",
    "--sync-raw-image-buffers",
    "--tessellation",
    "--trophy-duration",
    "--no-trophy-notifications",
    "--user-id",
    "--user-name",
    "--vblank-frequency",
    "--vr",
    "--vulkan-validation",
];

fn cli_switch_options() -> &'static [&'static str] {
    &[
        "--amd-cpu",
        "--fullscreen",
        "--hdr",
        "--hide-cursor",
        "--no-trophy-notifications",
        "--no-splash",
        "--playgo-hack",
        "--profile",
        "--rd",
        "--tessellation",
        "--vr",
    ]
}

fn cli_boolean_options() -> &'static [&'static str] {
    &[
        "--command-buffer-dump",
        "--gpu-assisted-validation",
        "--graphics-debug-dump",
        "--network-connected",
        "--readback-linear-images",
        "--perf-overlay",
        "--shader-validation",
        "--skip-notice-screen",
        "--spirv-debug-printf",
        "--sync-raw-image-buffers",
        "--vulkan-validation",
    ]
}

fn profile_capabilities(help: &str) -> ProfileCapabilities {
    let parsed: Vec<(&str, bool)> = help
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let flag = words.next()?;
            if !flag.starts_with("--")
                || !flag[2..]
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return None;
            }
            let takes_value = words
                .next()
                .is_some_and(|word| word.starts_with('<') || word.starts_with('['));
            Some((flag, takes_value))
        })
        .collect();
    let supported: HashSet<&str> = parsed.iter().map(|(flag, _)| *flag).collect();
    let reserved = ["--game", "--guest-exec", "--help"];
    let configurable = |flag: &&str| !reserved.contains(flag);
    let has = |flag: &str| supported.contains(flag);
    let options = parsed
        .iter()
        .map(|(flag, _)| *flag)
        .filter(configurable)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut option_metadata = cli_option_metadata(help);
    for metadata in option_metadata.values_mut() {
        metadata.state = CapabilityState::AdvertisedUnverified;
        metadata.state_explanation = "The executable advertises this option in --help, but Studio has not verified its implementation for this exact build.".into();
    }
    let mut option_states = HashMap::new();
    for flag in CONFIGURABLE_CLI_OPTIONS {
        option_states.insert(
            (*flag).to_owned(),
            CapabilityStatus {
                state: if has(flag) {
                    CapabilityState::AdvertisedUnverified
                } else {
                    CapabilityState::Unsupported
                },
                explanation: if has(flag) {
                    "Advertised by --help; implementation not source-verified for this build."
                } else {
                    "Not advertised by this build."
                }
                .into(),
            },
        );
    }
    for flag in &options {
        option_states
            .entry(flag.clone())
            .or_insert_with(|| CapabilityStatus {
                state: CapabilityState::AdvertisedUnverified,
                explanation:
                    "Advertised by --help; implementation not source-verified for this build."
                        .into(),
            });
    }
    let verified_options = Vec::new();
    ProfileCapabilities {
        screen_width: has("--screen-width"),
        screen_height: has("--screen-height"),
        present_mode: has("--present-mode"),
        perf_overlay: has("--perf-overlay"),
        fullscreen: has("--fullscreen"),
        controller_color: has("--controller-color"),
        controller_volume: has("--controller-volume"),
        controller_vibration: has("--controller-vibration"),
        keyboard_player: has("--keyboard-player"),
        options,
        verified_options,
        option_states,
        binary_sha256: String::new(),
        host_os: env::consts::OS.to_owned(),
        background_precompile: false,
        switch_options: parsed
            .iter()
            .filter(|(flag, takes_value)| !takes_value && configurable(flag))
            .map(|(flag, _)| (*flag).to_string())
            .collect(),
        option_metadata,
    }
}

fn cli_option_metadata(help: &str) -> HashMap<String, CliOptionMetadata> {
    let mut definitions = Vec::<(String, Option<String>, String)>::new();
    for line in help.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            let Some(flag_end) = trimmed.find(char::is_whitespace) else {
                continue;
            };
            let flag = &trimmed[..flag_end];
            if !flag[2..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                continue;
            }
            let mut remainder = trimmed[flag_end..].trim_start();
            let mut value_token = None;
            if remainder.starts_with('<') || remainder.starts_with('[') {
                let closing = if remainder.starts_with('<') { '>' } else { ']' };
                if let Some(end) = remainder.find(closing) {
                    value_token = Some(remainder[..=end].to_string());
                    remainder = remainder[end + 1..].trim_start();
                }
            }
            definitions.push((flag.to_string(), value_token, remainder.to_string()));
        } else if !trimmed.is_empty() {
            if let Some((_, _, description)) = definitions.last_mut() {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(trimmed);
            }
        }
    }

    definitions
        .into_iter()
        .map(|(flag, value_token, description)| {
            let token_value = value_token
                .as_deref()
                .unwrap_or_default()
                .trim_start_matches(['<', '['])
                .trim_end_matches(['>', ']']);
            let mut choices = token_value
                .split('|')
                .map(str::trim)
                .filter(|choice| !choice.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let mut minimum = None;
            let mut maximum = None;
            if choices.len() < 2 {
                choices.clear();
                let range = token_value
                    .split_once('-')
                    .or_else(|| token_value.split_once('–'));
                if let Some((low, high)) = range {
                    minimum = low.trim().parse().ok();
                    maximum = high.trim().parse().ok();
                }
            }
            let placeholder = token_value.to_ascii_lowercase();
            if choices.is_empty()
                && matches!(placeholder.as_str(), "value" | "mode" | "type" | "option")
            {
                choices = choices_from_description(&description);
            }
            let kind = if choices.len() >= 2 {
                if choices.len() == 2
                    && ((choices[0] == "true" && choices[1] == "false")
                        || (choices[0] == "false" && choices[1] == "true"))
                {
                    "bool"
                } else {
                    "enum"
                }
            } else if minimum.is_some() && maximum.is_some()
                || matches!(
                    placeholder.as_str(),
                    "num"
                        | "number"
                        | "index"
                        | "seconds"
                        | "duration"
                        | "volume"
                        | "frequency"
                        | "hz"
                        | "id"
                        | "percent"
                )
            {
                "number"
            } else if ["path", "file", "dir", "directory", "folder", "json"]
                .contains(&placeholder.as_str())
            {
                "path"
            } else if placeholder == "mapping" {
                "keymap"
            } else {
                "text"
            };
            let default_value = description
                .split_once("Default:")
                .and_then(|(_, value)| value.split(['.', ')']).next())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            (
                flag,
                CliOptionMetadata {
                    kind: kind.to_string(),
                    choices,
                    minimum,
                    maximum,
                    default_value,
                    description,
                    state: CapabilityState::AdvertisedUnverified,
                    state_explanation:
                        "Advertised by --help; implementation not source-verified for this build."
                            .into(),
                },
            )
        })
        .collect()
}

fn choices_from_description(description: &str) -> Vec<String> {
    let sentence = description.split('.').next().unwrap_or_default().trim();
    let normalized = sentence
        .replace(", or ", "|")
        .replace(" or ", "|")
        .replace(", and ", "|")
        .replace(" and ", "|")
        .replace(", ", "|");
    let choices = normalized
        .split('|')
        .map(str::trim)
        .filter(|choice| !choice.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if (2..=8).contains(&choices.len())
        && choices.iter().all(|choice| {
            choice.len() <= 32
                && choice.chars().all(|character| {
                    character.is_ascii_alphanumeric() || " _-/#".contains(character)
                })
        })
    {
        choices
    } else {
        Vec::new()
    }
}

fn build_for_game(c: &Connection, game_id: i64) -> Result<Build, String> {
    let g = game(c, game_id)?;
    let id = if let Some(id) = g.build_id {
        id
    } else {
        c.query_row("SELECT id FROM builds WHERE active=1 LIMIT 1", [], |r| {
            r.get(0)
        })
        .map_err(|_| "Select an active emulator build".to_string())?
    };
    build(c, id)
}

fn capabilities_for_build(b: &Build, repository: &Repo) -> Result<ProfileCapabilities, String> {
    let exe = emulator_executable(Path::new(&b.path));
    let canonical = fs::canonicalize(&exe).map_err(|e| e.to_string())?;
    let binary_bytes = fs::read(&canonical)
        .map_err(|e| format!("Could not hash selected emulator binary: {e}"))?;
    let binary_sha256 = format!("{:x}", Sha256::digest(&binary_bytes));
    let source_identity = source_matches_build(b, repository)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unverified-source".into());
    let key = format!(
        "{binary_sha256}:{}:{}:{source_identity}",
        env::consts::OS,
        b.commit_sha
    );
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, ProfileCapabilities>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(capabilities) = cache.lock().map_err(|e| e.to_string())?.get(&key).cloned() {
        return Ok(capabilities);
    }
    let mut capabilities = profile_capabilities(&emulator_help(&canonical)?);
    capabilities.binary_sha256 = binary_sha256;
    capabilities.verified_options = source_verified_options(b, repository, &capabilities.options);
    capabilities.background_precompile = source_proves_background_precompile(b, repository);
    // HDR is gated by the connected display and Vulkan surface support. Studio
    // cannot probe that combination reliably on every host, so it remains unavailable.
    capabilities.verified_options.retain(|flag| flag != "--hdr");
    let verified: HashSet<_> = capabilities.verified_options.iter().cloned().collect();
    capabilities.screen_width = verified.contains("--screen-width");
    capabilities.screen_height = verified.contains("--screen-height");
    capabilities.present_mode = verified.contains("--present-mode");
    capabilities.perf_overlay = verified.contains("--perf-overlay");
    capabilities.fullscreen = verified.contains("--fullscreen");
    capabilities.controller_color = verified.contains("--controller-color");
    capabilities.controller_volume = verified.contains("--controller-volume");
    capabilities.controller_vibration = verified.contains("--controller-vibration");
    capabilities.keyboard_player = verified.contains("--keyboard-player");
    for (flag, status) in &mut capabilities.option_states {
        if verified.contains(flag) {
            status.state = CapabilityState::VerifiedSupported;
            status.explanation = format!(
                "Verified in clean source revision {} and advertised by this binary (SHA-256 {}).",
                b.commit_sha,
                &capabilities.binary_sha256[..12]
            );
        }
    }
    for flag in &capabilities.options {
        if !verified.contains(flag) {
            if let Some(explanation) = source_option_unavailable_on_host(b, repository, flag) {
                capabilities.option_states.insert(
                    flag.clone(),
                    CapabilityStatus {
                        state: CapabilityState::Unavailable,
                        explanation: explanation.clone(),
                    },
                );
                if let Some(metadata) = capabilities.option_metadata.get_mut(flag) {
                    metadata.state = CapabilityState::Unavailable;
                    metadata.state_explanation = explanation;
                }
            }
        }
    }
    if capabilities.options.iter().any(|flag| flag == "--hdr") {
        let explanation = "This option also requires an HDR-capable connected display and Vulkan surface. Studio cannot verify both for this session.".to_string();
        capabilities.option_states.insert(
            "--hdr".into(),
            CapabilityStatus {
                state: CapabilityState::Unavailable,
                explanation: explanation.clone(),
            },
        );
        if let Some(metadata) = capabilities.option_metadata.get_mut("--hdr") {
            metadata.state = CapabilityState::Unavailable;
            metadata.state_explanation = explanation;
        }
    }
    for (flag, metadata) in &mut capabilities.option_metadata {
        if verified.contains(flag) {
            metadata.state = CapabilityState::VerifiedSupported;
            metadata.state_explanation = format!(
                "Source implementation matched build commit {}; binary SHA-256 {}.",
                b.commit_sha,
                &capabilities.binary_sha256[..12]
            );
        }
    }
    let mut cache = cache.lock().map_err(|e| e.to_string())?;
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(key, capabilities.clone());
    Ok(capabilities)
}

fn source_matches_build(b: &Build, repository: &Repo) -> Option<PathBuf> {
    let source = PathBuf::from(repository.source_path.as_deref()?);
    if git(Some(&source), &["rev-parse", "HEAD"]).ok()?.trim() != b.commit_sha
        || !git(Some(&source), &["status", "--porcelain"])
            .ok()?
            .trim()
            .is_empty()
    {
        return None;
    }
    Some(source)
}

fn source_option_unavailable_on_host(b: &Build, repository: &Repo, flag: &str) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let source = source_matches_build(b, repository)?;
    let main = fs::read_to_string(source.join("src/main.cpp")).ok()?;
    let mut windows_only = false;
    let mut found = false;
    for line in main.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#if") && trimmed.contains("KYTY_PLATFORM_WINDOWS") {
            windows_only = true;
        }
        if line.contains(flag) && windows_only {
            found = true;
        }
        if trimmed.starts_with("#endif") {
            windows_only = false;
        }
    }
    found.then(|| "This option is compiled only for Windows builds.".into())
}

fn source_flag_field(main: &str, flag: &str) -> Option<String> {
    for line in main.lines().filter(|line| line.contains(flag)) {
        if let Some((_, tail)) = line.split_once("ConfigOptions::") {
            let field = tail
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>();
            if !field.is_empty() {
                return Some(field);
            }
        }
    }
    let lines = main.lines().collect::<Vec<_>>();
    for (index, line) in lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains(flag))
    {
        for candidate in lines.iter().skip(index).take(14) {
            let token = candidate
                .split("options.config.")
                .nth(1)
                .or_else(|| candidate.split("options.").nth(1));
            if let Some(tail) = token {
                let field = tail
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<String>();
                if field.len() > 1 {
                    return Some(field);
                }
            }
            if candidate.contains("} else if (arg ==") && candidate != line {
                break;
            }
        }
    }
    None
}

fn source_verified_options(b: &Build, repository: &Repo, advertised: &[String]) -> Vec<String> {
    let Some(source) = source_matches_build(b, repository) else {
        return Vec::new();
    };
    let main_path = source.join("src/main.cpp");
    let Ok(main) = fs::read_to_string(&main_path) else {
        return Vec::new();
    };
    let mut other_source = String::new();
    for entry in walkdir::WalkDir::new(source.join("src"))
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.path() == main_path
            || !entry.file_type().is_file()
            || !entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| matches!(ext, "cpp" | "h" | "hpp" | "inl"))
        {
            continue;
        }
        if let Ok(text) = fs::read_to_string(entry.path()) {
            other_source.push_str(&text);
            other_source.push('\n');
        }
    }
    let windows_guarded = |flag: &str| {
        let mut windows_only = false;
        let mut found = false;
        for line in main.lines() {
            let t = line.trim();
            if t.starts_with("#if") && t.contains("KYTY_PLATFORM_WINDOWS") {
                windows_only = true;
            }
            if line.contains(flag) && windows_only {
                found = true;
            }
            if t.starts_with("#endif") {
                windows_only = false;
            }
        }
        found && !cfg!(windows)
    };
    advertised
        .iter()
        .filter(|flag| {
            if windows_guarded(flag) {
                return false;
            }
            let Some(field) = source_flag_field(&main, flag) else {
                return false;
            };
            let help = main
                .split("static void PrintUsage()")
                .last()
                .unwrap_or("")
                .split("static bool NextArg")
                .next()
                .unwrap_or("");
            help.contains(*flag) && other_source.contains(&field)
        })
        .cloned()
        .collect()
}

fn source_proves_background_precompile(b: &Build, repository: &Repo) -> bool {
    let Some(source) = source_matches_build(b, repository) else {
        return false;
    };
    let mut matches = 0;
    for entry in walkdir::WalkDir::new(source.join("src"))
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "cpp" | "h" | "hpp" | "inl"))
            && fs::read_to_string(entry.path()).is_ok_and(|s| {
                s.contains("KYTY_SHADER_PRECOMPILE") && s.contains("KYTY_PIPELINE_FAST_FIRST")
            })
        {
            matches += 1;
        }
    }
    matches > 0
}
fn profile(c: &Connection, game_id: i64) -> Result<Profile, String> {
    let mut profile = c.query_row(
        "SELECT screen_width,screen_height,present_mode,perf_overlay,fullscreen,controller_color,controller_volume,controller_vibration,keyboard_player,options_json FROM profiles WHERE game_id=?",
        [game_id],
        |r| {
            Ok(Profile {
                screen_width: r.get(0)?,
                screen_height: r.get(1)?,
                present_mode: r.get(2)?,
                perf_overlay: r.get::<_, i64>(3)? != 0,
                fullscreen: r.get::<_, i64>(4)? != 0,
                controller_color: r.get(5)?, controller_volume: r.get(6)?, controller_vibration: r.get(7)?, keyboard_player: r.get(8)?,
                options: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .unwrap_or_default();
    merge_legacy_profile_options(&mut profile);
    Ok(profile)
}
fn global_profile_options(c: &Connection) -> Result<BTreeMap<String, String>, String> {
    let json: String = c
        .query_row(
            "SELECT options_json FROM global_profile_defaults WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&json).unwrap_or_default())
}
fn build_profile_options(
    c: &Connection,
    build_id: i64,
) -> Result<BTreeMap<String, String>, String> {
    let json = c
        .query_row(
            "SELECT options_json FROM build_profile_defaults WHERE build_id=?",
            [build_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(json
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default())
}
fn validate_extra_profile_options(
    options: &BTreeMap<String, String>,
    capabilities: &ProfileCapabilities,
) -> Result<(), String> {
    if options
        .get("--no-trophy-notifications")
        .is_some_and(|value| value == "true")
        && options.contains_key("--trophy-duration")
    {
        return Err("Trophy duration cannot be set when trophy notifications are disabled".into());
    }
    for (flag, value) in options {
        if !capabilities
            .verified_options
            .iter()
            .any(|supported| supported == flag)
        {
            let detail = capabilities
                .option_states
                .get(flag)
                .map(|status| status.explanation.as_str())
                .unwrap_or("No source-verified capability entry exists for this build.");
            return Err(format!("Cannot apply {flag}: {detail}"));
        }
        if let Some(metadata) = capabilities.option_metadata.get(flag) {
            if !metadata.choices.is_empty() && !metadata.choices.contains(value) {
                return Err(format!(
                    "{flag} must be one of: {}",
                    metadata.choices.join(", ")
                ));
            }
            if metadata.minimum.is_some() || metadata.maximum.is_some() {
                let number = value
                    .parse::<i64>()
                    .map_err(|_| format!("{flag} must be a number"))?;
                if metadata.minimum.is_some_and(|minimum| number < minimum)
                    || metadata.maximum.is_some_and(|maximum| number > maximum)
                {
                    return Err(format!(
                        "{flag} must be between {} and {}",
                        metadata
                            .minimum
                            .map_or_else(|| "any value".into(), |n| n.to_string()),
                        metadata
                            .maximum
                            .map_or_else(|| "any value".into(), |n| n.to_string())
                    ));
                }
            }
        }
        if !CONFIGURABLE_CLI_OPTIONS.contains(&flag.as_str()) {
            if flag.len() > 128
                || !flag.strip_prefix("--").is_some_and(|name| {
                    !name.is_empty()
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
            {
                return Err(format!("Invalid emulator option name: {flag}"));
            }
            if capabilities.switch_options.contains(flag) {
                if !["true", "false"].contains(&value.as_str()) {
                    return Err(format!("{flag} must be true or false"));
                }
            } else if value.trim().is_empty() || value.len() > 4096 {
                return Err(format!("{flag} must have a value between 1 and 4096 bytes"));
            }
            continue;
        }
        if cli_switch_options().contains(&flag.as_str()) {
            if !["true", "false"].contains(&value.as_str()) {
                return Err(format!("{flag} must be true or false"));
            }
            continue;
        }
        if cli_boolean_options().contains(&flag.as_str()) {
            if !["true", "false"].contains(&value.as_str()) {
                return Err(format!("{flag} must be true or false"));
            }
            continue;
        }
        match flag.as_str() {
            "--screen-width" | "--screen-height" => {
                let pixels = value
                    .parse::<u32>()
                    .map_err(|_| format!("{flag} must be a positive 32-bit integer"))?;
                if pixels == 0 {
                    return Err(format!("{flag} must be greater than zero"));
                }
            }
            "--present-mode" => {
                let supported_modes = capabilities
                    .option_metadata
                    .get(flag)
                    .map(|metadata| metadata.choices.as_slice())
                    .filter(|choices| !choices.is_empty());
                if supported_modes.map_or_else(
                    || !["Fifo", "Mailbox", "Immediate"].contains(&value.as_str()),
                    |choices| !choices.iter().any(|choice| choice == value),
                ) {
                    return Err("Present mode must be Fifo, Mailbox, or Immediate".into());
                }
            }
            "--controller-color" => {
                if value.len() != 7
                    || !value.starts_with('#')
                    || !value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
                {
                    return Err("Controller color must use #RRGGBB format".into());
                }
            }
            "--controller-volume" | "--controller-vibration" => {
                let amount = value
                    .parse::<u32>()
                    .map_err(|_| format!("{flag} must be between 0 and 100"))?;
                if amount > 100 {
                    return Err(format!("{flag} must be between 0 and 100"));
                }
            }
            "--keyboard-player" => {
                let player = value
                    .parse::<u32>()
                    .map_err(|_| "Keyboard player must be between 0 and 4".to_string())?;
                if player > 4 {
                    return Err("Keyboard player must be between 0 and 4".into());
                }
            }
            "--gpu" => {
                let index = value
                    .parse::<i32>()
                    .map_err(|_| "GPU index must be a signed integer".to_string())?;
                if index < -1 {
                    return Err("GPU index must be -1 for automatic selection or a non-negative device index".into());
                }
            }
            "--vblank-frequency" => {
                let frequency = value
                    .parse::<u32>()
                    .map_err(|_| "VBlank frequency must be between 30 and 360 Hz".to_string())?;
                if !(30..=360).contains(&frequency) {
                    return Err("VBlank frequency must be between 30 and 360 Hz".into());
                }
            }
            "--trophy-duration" => {
                let seconds = value
                    .parse::<u32>()
                    .map_err(|_| "Trophy duration must be an integer".to_string())?;
                if !(5..=10).contains(&seconds) {
                    return Err("Trophy duration must be between 5 and 10 seconds".into());
                }
            }
            "--console-language" => {
                let language = value
                    .parse::<u32>()
                    .map_err(|_| "Console language must be between 0 and 29".to_string())?;
                if language > 29 {
                    return Err("Console language must be between 0 and 29".into());
                }
            }
            "--shader-optimization-type" => {
                let supported_modes = capabilities
                    .option_metadata
                    .get(flag)
                    .map(|metadata| metadata.choices.as_slice())
                    .filter(|choices| !choices.is_empty());
                if supported_modes.map_or_else(
                    || !["None", "Size", "Performance"].contains(&value.as_str()),
                    |choices| !choices.iter().any(|choice| choice == value),
                ) {
                    return Err("Shader optimization must be None, Size, or Performance".into());
                }
            }
            "--shader-log-direction" | "--printf-direction" => {
                let supported_modes = capabilities
                    .option_metadata
                    .get(flag)
                    .map(|metadata| metadata.choices.as_slice())
                    .filter(|choices| !choices.is_empty());
                if supported_modes.map_or_else(
                    || !["Silent", "Console", "File"].contains(&value.as_str()),
                    |choices| !choices.iter().any(|choice| choice == value),
                ) {
                    return Err(format!("{flag} must be Silent, Console, or File"));
                }
            }
            "--user-name" => {
                if value.is_empty() || value.len() > 16 {
                    return Err("Username must contain 1 to 16 UTF-8 bytes".into());
                }
            }
            "--user-id" => {
                let id = value
                    .parse::<i32>()
                    .map_err(|_| "User ID must be a valid signed integer".to_string())?;
                let max_user_id = i32::MAX - 3 * 0x100;
                if id < 0 || id > max_user_id || id == 0xfe || id == 0xff {
                    return Err(format!(
                        "User ID must be between 0 and {max_user_id} and cannot use the reserved IDs 254 or 255"
                    ));
                }
            }
            "--keymap" => {
                let mappings = value.lines().filter(|line| !line.trim().is_empty());
                let mut count = 0;
                for mapping in mappings {
                    let Some((control, input)) = mapping.split_once('=') else {
                        return Err("Each controller mapping must use Control=Input format".into());
                    };
                    if control.trim().is_empty() || input.trim().is_empty() {
                        return Err(
                            "Controller mappings cannot have an empty control or input".into()
                        );
                    }
                    count += 1;
                }
                if count == 0 {
                    return Err("Add at least one controller mapping or reset the option".into());
                }
            }
            "--mic" => {
                if value.trim().is_empty() || value.len() > 256 {
                    return Err("Microphone name must contain 1 to 256 bytes".into());
                }
            }
            "--game-patch" => {
                let path = fs::canonicalize(value)
                    .map_err(|_| format!("{flag} must point to an existing file"))?;
                if !path.is_file() {
                    return Err(format!("{flag} must point to an existing file"));
                }
            }
            "--printf-output-file" => {
                let path = Path::new(value);
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                if path.file_name().is_none() || !parent.is_dir() {
                    return Err(
                        "Guest printf output must be a file inside an existing directory".into(),
                    );
                }
                if path
                    .symlink_metadata()
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    return Err("Guest printf output cannot be a symbolic link".into());
                }
                if path.exists() && !path.is_file() {
                    return Err("Guest printf output path must be a file".into());
                }
            }
            "--addcont-dir" | "--shader-log-folder" | "--command-buffer-dump-folder" => {
                let path = fs::canonicalize(value)
                    .map_err(|_| format!("{flag} must point to an existing directory"))?;
                if !path.is_dir() {
                    return Err(format!("{flag} must point to an existing directory"));
                }
            }
            _ => {
                if value.trim().is_empty() {
                    return Err(format!("{flag} cannot be empty"));
                }
            }
        }
    }
    if options
        .get("--gpu-assisted-validation")
        .is_some_and(|v| v == "true")
        && options
            .get("--vulkan-validation")
            .is_some_and(|v| v == "false")
    {
        return Err("GPU-assisted validation requires Vulkan validation".into());
    }
    if options
        .get("--gpu-assisted-validation")
        .is_some_and(|v| v == "true")
        && !capabilities
            .verified_options
            .iter()
            .any(|v| v == "--vulkan-validation")
    {
        return Err(
            "GPU-assisted validation requires Vulkan validation support in this emulator".into(),
        );
    }
    Ok(())
}
fn validate_gpu_hardware_selection(
    options: &BTreeMap<String, String>,
    vulkan_details: &str,
) -> Result<(), String> {
    let Some(value) = options.get("--gpu") else {
        return Ok(());
    };
    let index = value
        .parse::<i32>()
        .map_err(|_| "GPU index must be a signed integer".to_string())?;
    if index < 0 {
        return Ok(());
    }
    let available = vulkan_details.lines().any(|line| {
        line.trim()
            .strip_prefix("GPU")
            .and_then(|value| value.trim().trim_end_matches(':').parse::<i32>().ok())
            == Some(index)
    });
    if available {
        Ok(())
    } else {
        Err(format!(
            "GPU {index} is not reported by Vulkan on this system. Select automatic GPU choice or inspect graphics diagnostics."
        ))
    }
}
fn effective_profile_options(
    global: &BTreeMap<String, String>,
    game: &BTreeMap<String, String>,
    capabilities: &ProfileCapabilities,
) -> Result<BTreeMap<String, String>, String> {
    if let Some(unsupported) = game.keys().find(|flag| {
        !capabilities
            .verified_options
            .iter()
            .any(|supported| supported == *flag)
    }) {
        return Err(format!(
            "The selected emulator build no longer supports {unsupported}; clear unsupported game options before launching"
        ));
    }
    let mut effective: BTreeMap<_, _> = global
        .iter()
        .filter(|(flag, _)| {
            capabilities
                .verified_options
                .iter()
                .any(|supported| supported == *flag)
        })
        .map(|(flag, value)| (flag.clone(), value.clone()))
        .collect();
    effective.extend(
        game.iter()
            .map(|(flag, value)| (flag.clone(), value.clone())),
    );
    if effective
        .get("--no-trophy-notifications")
        .is_some_and(|value| value == "true")
    {
        effective.remove("--trophy-duration");
    }
    validate_extra_profile_options(&effective, capabilities)?;
    Ok(effective)
}
fn canonicalize_option_path(flag: &str, value: &str) -> Result<String, String> {
    if flag == "--printf-output-file" {
        let path = Path::new(value);
        let name = path
            .file_name()
            .ok_or("Guest printf output needs a file name")?;
        let parent_path = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent =
            fs::canonicalize(parent_path).map_err(|e| format!("Invalid output folder: {e}"))?;
        return Ok(parent.join(name).display().to_string());
    }
    fs::canonicalize(value)
        .map(|path| path.display().to_string())
        .map_err(|e| format!("Invalid path for {flag}: {e}"))
}
fn extra_profile_arguments(
    options: &BTreeMap<String, String>,
    dynamic_switches: &[String],
) -> Vec<OsString> {
    let mut args = Vec::new();
    if options
        .get("--gpu-assisted-validation")
        .is_some_and(|value| value == "true")
        && !options.contains_key("--vulkan-validation")
    {
        args.push(OsString::from("--vulkan-validation"));
        args.push(OsString::from("true"));
    }
    for (flag, value) in options {
        if cli_switch_options().contains(&flag.as_str())
            || dynamic_switches.iter().any(|switch| switch == flag)
        {
            if value == "true" {
                args.push(flag.into());
            }
        } else if flag == "--keymap" {
            for mapping in value.lines().filter(|line| !line.trim().is_empty()) {
                args.push(flag.into());
                args.push(mapping.trim().into());
            }
        } else {
            args.push(flag.into());
            args.push(value.into());
        }
    }
    args
}
#[derive(Serialize)]
struct SystemInfo {
    os: String,
    arch: String,
    cpu: String,
    memory_total_bytes: u64,
    memory_available_bytes: u64,
    controllers: Vec<String>,
    session: String,
    gpu: String,
    dependencies: Vec<Dependency>,
    paths: HashMap<String, String>,
}
#[derive(Serialize)]
struct GraphicsDiagnostics {
    vulkan_status: String,
    vulkan_details: String,
    drivers: Vec<String>,
    monitors: Vec<String>,
}
#[derive(Serialize)]
struct Dependency {
    name: String,
    present: bool,
}
#[derive(Serialize, Clone, PartialEq, Eq)]
struct ShaderPrecompileProgress {
    shaders_total: Option<u64>,
    pipelines_compiled: u64,
    pipelines_total: Option<u64>,
    live_workers: u64,
}
#[derive(Serialize, Clone)]
struct JobEvent {
    job_id: u64,
    timestamp_ms: u64,
    channel: String,
    kind: String,
    phase: String,
    line: String,
    done: bool,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<ShaderPrecompileProgress>,
}
#[derive(Serialize, Clone)]
struct GameScanEvent {
    scan_id: u64,
    phase: String,
    completed: u64,
    total: u64,
    current: String,
    done: bool,
}
#[derive(Serialize)]
struct GameScanResult {
    imported: usize,
    already_registered: usize,
    rejected: usize,
}
static NEXT_GAME_SCAN_ID: AtomicU64 = AtomicU64::new(1);
static GAME_SIZE_REFRESH_RUNNING: AtomicBool = AtomicBool::new(false);
static SHADER_PRECOMPILE_ACTIVE: AtomicBool = AtomicBool::new(false);
static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVE_JOB_ID: AtomicU64 = AtomicU64::new(0);
static SESSION_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());
struct Jobs {
    pid: AtomicI32,
    active: AtomicBool,
    updating: AtomicBool,
    job_id: AtomicU64,
    event_kind: Mutex<String>,
    cancel_requested: AtomicBool,
    cancellable: AtomicBool,
    control: Mutex<()>,
}
fn begin_job(
    jobs: &Jobs,
    message: &str,
    cancellable: bool,
    event_kind: &str,
) -> Result<u64, String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if jobs.updating.load(Ordering::SeqCst) {
        return Err("KytyPS5 Studio is preparing an application update".into());
    }
    if jobs.active.swap(true, Ordering::SeqCst) {
        return Err(message.to_string());
    }
    jobs.cancel_requested.store(false, Ordering::SeqCst);
    jobs.cancellable.store(cancellable, Ordering::SeqCst);
    jobs.pid.store(0, Ordering::SeqCst);
    *jobs.event_kind.lock().map_err(|error| error.to_string())? = event_kind.to_string();
    let job_id = NEXT_JOB_ID.fetch_add(1, Ordering::SeqCst).max(1);
    jobs.job_id.store(job_id, Ordering::SeqCst);
    ACTIVE_JOB_ID.store(job_id, Ordering::SeqCst);
    Ok(job_id)
}
#[cfg(unix)]
fn terminate_process_group(pid: i32) -> std::io::Result<()> {
    let result = unsafe { libc::kill(-pid, libc::SIGTERM) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[cfg(unix)]
fn process_group_exists(pgid: i32) -> bool {
    if Path::new("/proc").is_dir() {
        return fs::read_dir("/proc").ok().is_some_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                    return false;
                };
                proc_stat(pid).is_some_and(|stat| {
                    stat.process_group == i64::from(pgid) && !matches!(stat.state, 'Z' | 'X')
                })
            })
        });
    }
    let result = unsafe { libc::kill(-pgid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
#[cfg(target_os = "linux")]
fn child_exit_observed(pid: u32) -> Result<bool, String> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    loop {
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == 0 {
            let signal = unsafe { *(std::ptr::addr_of!(info).cast::<libc::c_int>()) };
            return Ok(signal == libc::SIGCHLD);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return Err(format!("Could not inspect child process status: {error}"));
    }
}
#[cfg(unix)]
fn stop_process_group(pid: i32) -> Result<(), String> {
    // The caller has verified that pid is the session leader for this exact
    // process incarnation. Launches use process_group(0), so this group is
    // owned by the emulator session rather than the Studio process.
    if let Err(error) = terminate_process_group(pid) {
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        return Err(format!("Could not send SIGTERM: {error}"));
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if !process_group_exists(pid) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            // A process-group ID cannot be reused while any member of that
            // group remains, so a positive group probe still identifies the
            // isolated session even if its leader exited before its children.
            let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(format!(
                        "Could not force-stop emulator process group: {error}"
                    ));
                }
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn attach_owned_child(child: &mut std::process::Child) -> Result<(), String> {
    #[cfg(windows)]
    {
        if let Err(error) = windows_process::attach_child(child.id()) {
            let kill = child.kill().err();
            let wait = child.wait().err();
            return Err(format!(
                "Could not establish Windows process-tree ownership: {error}{}{}",
                kill.map(|error| format!("; child kill failed: {error}"))
                    .unwrap_or_default(),
                wait.map(|error| format!("; child wait failed: {error}"))
                    .unwrap_or_default()
            ));
        }
    }
    #[cfg(not(windows))]
    let _ = child;
    Ok(())
}
/// Windows children must not run before they are assigned to their
/// kill-on-close Job Object. `attach_owned_child` resumes the primary thread
/// only after assignment succeeds, so every managed spawn must set
/// CREATE_SUSPENDED before calling `Command::spawn`.
fn prepare_owned_child(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_SUSPENDED: u32 = 0x0000_0004;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // `creation_flags` replaces the previously requested custom flags.
        // Preserve hidden-window and process-group flags while ensuring the
        // process cannot run before Job Object assignment.
        command.creation_flags(CREATE_SUSPENDED | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}
#[cfg(windows)]
fn stop_process_group(pid: i32) -> Result<(), String> {
    let pid = windows_pid_from_job_slot(pid).ok_or("Invalid Windows process ID")?;
    windows_process::terminate_child(pid)
}
#[cfg(windows)]
fn release_owned_child(pid: i32) {
    if let Some(pid) = windows_pid_from_job_slot(pid) {
        windows_process::release_child(pid);
    }
}
#[cfg(any(windows, test))]
fn windows_pid_from_job_slot(pid: i32) -> Option<u32> {
    (pid != 0).then_some(pid as u32)
}
fn stop_and_reap_owned_child(
    child: &mut std::process::Child,
) -> (Result<(), String>, Result<std::process::ExitStatus, String>) {
    let pid = child.id() as i32;
    let cleanup = stop_process_group(pid);
    let wait = child.wait().map_err(|error| error.to_string());
    #[cfg(windows)]
    release_owned_child(pid);
    (cleanup, wait)
}
#[cfg(target_os = "linux")]
fn wait_for_child_and_cleanup_group(
    child: &mut std::process::Child,
    group_cleaned: impl FnOnce(),
) -> Result<(std::process::ExitStatus, Result<(), String>), String> {
    let pid = child.id() as i32;
    loop {
        match child_exit_observed(pid as u32) {
            Ok(true) => break,
            Ok(false) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let cleanup = stop_process_group(pid);
                group_cleaned();
                let _ = child.wait();
                return Err(match cleanup {
                    Ok(()) => error,
                    Err(cleanup_error) => {
                        format!("{error}; process cleanup failed: {cleanup_error}")
                    }
                });
            }
        }
    }
    let cleanup = stop_process_group(pid);
    group_cleaned();
    match child.wait() {
        Ok(status) => Ok((status, cleanup)),
        Err(wait_error) => Err(match cleanup {
            Ok(()) => format!("Could not reap emulator process: {wait_error}"),
            Err(cleanup_error) => format!(
                "Could not reap emulator process ({wait_error}) or clean its process group ({cleanup_error})"
            ),
        }),
    }
}
#[cfg(not(target_os = "linux"))]
fn wait_for_child_and_cleanup_group(
    child: &mut std::process::Child,
    group_cleaned: impl FnOnce(),
) -> Result<(std::process::ExitStatus, Result<(), String>), String> {
    let pid = child.id() as i32;
    let status = child.wait().map_err(|error| error.to_string());
    let cleanup = stop_process_group(pid);
    group_cleaned();
    #[cfg(windows)]
    release_owned_child(pid);
    status.map(|status| (status, cleanup))
}
#[derive(Default)]
struct ProcessSamples(Mutex<HashMap<i64, ProcessSample>>);
#[derive(Default)]
struct ProcessSample {
    previous: Option<(u64, Instant)>,
    missing_samples: u8,
}
const PROCESS_MISSING_SAMPLE_LIMIT: u8 = 5;

fn record_lost_session(
    connection: &mut Connection,
    session_id: i64,
    ended_at: i64,
    started_at: Option<i64>,
    outcome: &str,
) -> Result<bool, String> {
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let changed = transaction
        .execute(
            "UPDATE sessions SET ended_at=?,outcome=? WHERE id=? AND ended_at IS NULL",
            params![ended_at, outcome, session_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 1 {
        if let Some(started_at) = started_at {
            transaction
                .execute(
                    "UPDATE games SET play_seconds=play_seconds+?,last_played=? WHERE id=(SELECT game_id FROM sessions WHERE id=?)",
                    params![ended_at.saturating_sub(started_at).max(0), started_at, session_id],
                )
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(changed == 1)
}

const SESSION_LOG_MAX_BYTES: u64 = 32 * 1024 * 1024;
const SESSION_LOG_TRUNCATION_MARKER: &[u8] =
    b"\n[Studio] Session log reached its 32 MiB limit; further output was omitted.\n";
const BUILD_LOG_MAX_BYTES: u64 = 32 * 1024 * 1024;
const BUILD_LOG_TRUNCATION_MARKER: &[u8] =
    b"\n[Studio] Build log reached its 32 MiB limit; further output was omitted.\n";
struct BuildLogWriter {
    file: fs::File,
    max_bytes: u64,
    written: u64,
    truncated: bool,
}
impl BuildLogWriter {
    fn new(file: fs::File, max_bytes: u64) -> Self {
        let written = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        let max_bytes = max_bytes.max(BUILD_LOG_TRUNCATION_MARKER.len() as u64);
        Self {
            file,
            max_bytes,
            written,
            truncated: written >= max_bytes,
        }
    }
    fn append_line(&mut self, stream: &str, line: &str) -> std::io::Result<()> {
        if self.truncated {
            return Ok(());
        }
        let record = format!("[{stream}] {line}\n");
        let content_limit = self
            .max_bytes
            .saturating_sub(BUILD_LOG_TRUNCATION_MARKER.len() as u64);
        let remaining = content_limit.saturating_sub(self.written) as usize;
        let bytes = record.as_bytes();
        let count = bytes.len().min(remaining);
        self.file.write_all(&bytes[..count])?;
        self.written += count as u64;
        if count < bytes.len() {
            self.file.write_all(BUILD_LOG_TRUNCATION_MARKER)?;
            self.written += BUILD_LOG_TRUNCATION_MARKER.len() as u64;
            self.truncated = true;
        }
        Ok(())
    }
}
struct SessionLogWriter {
    file: fs::File,
    max_bytes: u64,
    written: u64,
    truncated: bool,
}
impl SessionLogWriter {
    fn new(file: fs::File, max_bytes: u64) -> Self {
        Self {
            file,
            max_bytes: max_bytes.max(SESSION_LOG_TRUNCATION_MARKER.len() as u64),
            written: 0,
            truncated: false,
        }
    }
    fn append_line(&mut self, stream: &str, line: &str) -> std::io::Result<()> {
        if self.truncated {
            return Ok(());
        }
        let record = format!("[{stream}] {line}\n");
        let content_limit = self
            .max_bytes
            .saturating_sub(SESSION_LOG_TRUNCATION_MARKER.len() as u64);
        let remaining = content_limit.saturating_sub(self.written) as usize;
        let bytes = record.as_bytes();
        let count = bytes.len().min(remaining);
        self.file.write_all(&bytes[..count])?;
        self.written += count as u64;
        if count < bytes.len() {
            self.file.write_all(SESSION_LOG_TRUNCATION_MARKER)?;
            self.truncated = true;
        }
        Ok(())
    }
}
#[cfg(unix)]
fn set_nonblocking_fd(fd: i32) -> std::io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(unix)]
fn configure_session_output(child: &std::process::Child) -> std::io::Result<()> {
    child
        .stdout
        .as_ref()
        .map(|stream| set_nonblocking_fd(stream.as_raw_fd()))
        .transpose()?;
    child
        .stderr
        .as_ref()
        .map(|stream| set_nonblocking_fd(stream.as_raw_fd()))
        .transpose()?;
    Ok(())
}
#[cfg(windows)]
fn configure_session_output(_child: &std::process::Child) -> std::io::Result<()> {
    Ok(())
}
const PROCESS_OUTPUT_LINE_MAX_BYTES: usize = 64 * 1024;
struct CappedOutputLine {
    bytes: Vec<u8>,
    truncated: bool,
}
impl CappedOutputLine {
    fn text(self) -> String {
        let mut text = String::from_utf8_lossy(&self.bytes)
            .trim_end_matches(['\r', '\n'])
            .to_string();
        if self.truncated {
            text.push_str(" [line truncated at configured limit]");
        }
        text
    }
}
struct CappedOutputReader<R: Read> {
    reader: BufReader<R>,
    pending: Vec<u8>,
    max_bytes: usize,
    truncated: bool,
    eof: bool,
}
impl<R: Read> CappedOutputReader<R> {
    fn new(reader: R) -> Self {
        Self::with_limit(reader, PROCESS_OUTPUT_LINE_MAX_BYTES)
    }
    fn with_limit(reader: R, max_bytes: usize) -> Self {
        Self {
            reader: BufReader::new(reader),
            pending: Vec::with_capacity(4096),
            max_bytes: max_bytes.max(1),
            truncated: false,
            eof: false,
        }
    }
    fn take_pending(&mut self) -> Option<CappedOutputLine> {
        if self.pending.is_empty() && !self.truncated {
            return None;
        }
        Some(CappedOutputLine {
            bytes: std::mem::take(&mut self.pending),
            truncated: std::mem::take(&mut self.truncated),
        })
    }
    fn append(&mut self, bytes: &[u8]) {
        let remaining = self.max_bytes.saturating_sub(self.pending.len());
        self.pending
            .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        self.truncated |= bytes.len() > remaining;
    }
    fn next_line(&mut self) -> std::io::Result<Option<CappedOutputLine>> {
        if self.eof {
            return Ok(self.take_pending());
        }
        loop {
            let (bytes, consumed, complete, reached_eof) = {
                let available = self.reader.fill_buf()?;
                if available.is_empty() {
                    (Vec::new(), 0, false, true)
                } else if let Some(index) = available.iter().position(|byte| *byte == b'\n') {
                    let consumed = index + 1;
                    (available[..consumed].to_vec(), consumed, true, false)
                } else {
                    let consumed = available.len();
                    (available.to_vec(), consumed, false, false)
                }
            };
            if reached_eof {
                self.eof = true;
                return Ok(self.take_pending());
            }
            self.reader.consume(consumed);
            self.append(&bytes);
            if complete {
                return Ok(self.take_pending());
            }
        }
    }
}
fn spawn_session_output_reader<R>(
    stream: R,
    label: &'static str,
    log: Arc<Mutex<SessionLogWriter>>,
    app: tauri::AppHandle,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = CappedOutputReader::new(stream);
        let mut stopped_at = None;
        loop {
            if stop.load(Ordering::SeqCst) && stopped_at.is_none() {
                stopped_at = Some(Instant::now());
            }
            if stopped_at
                .is_some_and(|instant: Instant| instant.elapsed() >= Duration::from_millis(250))
            {
                if let Some(line) = reader.take_pending() {
                    write_session_output(&log, &app, label, line.text().as_bytes());
                }
                break;
            }
            match reader.next_line() {
                Ok(Some(line)) => {
                    write_session_output(&log, &app, label, line.text().as_bytes());
                }
                Ok(None) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if stopped_at.is_some() {
                        if let Some(line) = reader.take_pending() {
                            write_session_output(&log, &app, label, line.text().as_bytes());
                        }
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => {
                    let text = format!("output reader error: {error}");
                    let _ = log.lock().unwrap().append_line(label, &text);
                    break;
                }
            }
        }
    })
}
fn write_session_output(
    log: &Arc<Mutex<SessionLogWriter>>,
    app: &tauri::AppHandle,
    label: &str,
    bytes: &[u8],
) {
    let line = String::from_utf8_lossy(bytes);
    let line = line.trim_end_matches(['\r', '\n']);
    let _ = log.lock().unwrap().append_line(label, line);
    emit(app, "game", label, line.to_string(), false, true);
}
#[derive(Clone, Copy, Debug)]
struct ProcStat {
    state: char,
    process_group: i64,
    cpu_ticks: u64,
    start_ticks: u64,
    resident_bytes: u64,
}
#[cfg(unix)]
fn parse_proc_stat(contents: &str, page_size: u64) -> Option<ProcStat> {
    let close = contents.rfind(')')?;
    let fields: Vec<_> = contents.get(close + 1..)?.split_whitespace().collect();
    let state = fields.first()?.chars().next()?;
    let process_group = fields.get(2)?.parse::<i32>().ok()?;
    let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
    let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
    let start_ticks = fields.get(19)?.parse::<u64>().ok()?;
    let resident_pages = fields.get(21)?.parse::<i64>().ok()?.max(0) as u64;
    Some(ProcStat {
        state,
        process_group: i64::from(process_group),
        cpu_ticks: user_ticks.saturating_add(system_ticks),
        start_ticks,
        resident_bytes: resident_pages.saturating_mul(page_size),
    })
}
#[cfg(unix)]
fn proc_stat(pid: u32) -> Option<ProcStat> {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return None;
    }
    let contents = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_proc_stat(&contents, page_size as u64)
}
#[cfg(unix)]
fn process_executable_matches(pid: u32, expected_executable: &Path) -> bool {
    fs::canonicalize(format!("/proc/{pid}/exe"))
        .ok()
        .zip(fs::canonicalize(expected_executable).ok())
        .is_some_and(|(actual, expected)| actual == expected)
}
#[cfg(windows)]
fn proc_stat(pid: u32) -> Option<ProcStat> {
    let snapshot = windows_process::snapshot(pid)?;
    Some(ProcStat {
        state: if snapshot.running { 'R' } else { 'Z' },
        process_group: i64::from(pid),
        cpu_ticks: snapshot.cpu_time_100ns,
        start_ticks: snapshot.creation_time,
        resident_bytes: snapshot.resident_bytes,
    })
}
#[cfg(windows)]
fn process_executable_matches(pid: u32, expected_executable: &Path) -> bool {
    windows_process::matches_executable(pid, expected_executable)
}
fn verified_session_process(
    pid: i64,
    expected_start: Option<i64>,
    actual: ProcStat,
    executable_matches: bool,
) -> bool {
    let Some(expected_start) = expected_start.and_then(|ticks| u64::try_from(ticks).ok()) else {
        return false;
    };
    pid > 1
        && actual.process_group == pid
        && actual.start_ticks == expected_start
        && executable_matches
}
#[cfg(target_os = "linux")]
struct PidFd(std::os::fd::RawFd);
#[cfg(target_os = "linux")]
impl Drop for PidFd {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}
#[cfg(target_os = "linux")]
fn send_pidfd_signal(pidfd: &PidFd, signal: libc::c_int) -> std::io::Result<()> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd.0,
            signal,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[cfg(target_os = "linux")]
fn stop_verified_session_group(
    pid: i32,
    expected_start: Option<i64>,
    expected_executable: &Path,
) -> Result<(), String> {
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOSYS) {
            return Err(
                "Safe process-group stop requires Linux pidfd support (Linux 5.3 or newer)".into(),
            );
        }
        return Err(format!(
            "Could not safely pin the emulator process identity: {error}"
        ));
    }
    let pidfd = PidFd(descriptor as std::os::fd::RawFd);
    let actual = proc_stat(pid as u32).ok_or("The emulator process has already exited")?;
    if !verified_session_process(
        i64::from(pid),
        expected_start,
        actual,
        process_executable_matches(pid as u32, expected_executable),
    ) {
        return Err("The process identity or isolated process group no longer matches this registered emulator session".into());
    }

    // Freeze the pidfd-pinned group leader before using its numeric PGID. This
    // keeps that PGID reserved across the group signal, even if PID reuse is
    // possible between /proc inspection and signal delivery.
    send_pidfd_signal(&pidfd, libc::SIGSTOP)
        .map_err(|error| format!("Could not pin emulator process group: {error}"))?;
    let freeze_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match proc_stat(pid as u32) {
            Some(stat)
                if stat.start_ticks == actual.start_ticks && matches!(stat.state, 'T' | 't') =>
            {
                break;
            }
            Some(stat)
                if stat.start_ticks == actual.start_ticks && Instant::now() < freeze_deadline =>
            {
                thread::sleep(Duration::from_millis(2));
            }
            _ => {
                let _ = send_pidfd_signal(&pidfd, libc::SIGCONT);
                return Err("The emulator exited or could not be safely pinned before signaling its process group".into());
            }
        }
    }
    let terminate = unsafe { libc::kill(-pid, libc::SIGTERM) };
    if terminate != 0 {
        let error = std::io::Error::last_os_error();
        let _ = send_pidfd_signal(&pidfd, libc::SIGCONT);
        return Err(format!(
            "Could not send SIGTERM to emulator process group: {error}"
        ));
    }
    if let Err(error) = send_pidfd_signal(&pidfd, libc::SIGCONT) {
        let _ = send_pidfd_signal(&pidfd, libc::SIGKILL);
        return Err(format!(
            "Could not resume emulator for graceful shutdown: {error}"
        ));
    }
    stop_process_group(pid)
}
fn cpu_percent(
    previous_ticks: u64,
    current_ticks: u64,
    elapsed: Duration,
    ticks_per_second: f64,
) -> Option<f64> {
    let elapsed = elapsed.as_secs_f64();
    if elapsed <= 0.0 || ticks_per_second <= 0.0 {
        None
    } else {
        Some(
            current_ticks.saturating_sub(previous_ticks) as f64 / ticks_per_second / elapsed
                * 100.0,
        )
    }
}
fn state_paths(app: &tauri::AppHandle) -> Result<Paths, String> {
    Ok(app.state::<Paths>().inner().clone())
}
#[derive(Serialize)]
struct StorageCategory {
    name: String,
    path: String,
    size_bytes: u64,
}
#[derive(Serialize)]
struct StorageReport {
    categories: Vec<StorageCategory>,
}
fn directory_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}
fn has_generated_build_install_name(name: &str) -> bool {
    let mut parts = name.splitn(3, '-');
    let Some(commit) = parts.next() else {
        return false;
    };
    let Some(build_type) = parts.next() else {
        return false;
    };
    let Some(timestamp) = parts.next() else {
        return false;
    };
    commit.len() == 12
        && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        && ["Release", "Debug", "RelWithDebInfo"].contains(&build_type)
        && !timestamp.is_empty()
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
}
fn incomplete_build_installations(
    builds_root: &Path,
    registered_paths: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let Ok(root_metadata) = fs::symlink_metadata(builds_root) else {
        return Vec::new();
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Vec::new();
    }
    let Ok(canonical_root) = fs::canonicalize(builds_root) else {
        return Vec::new();
    };
    let Ok(repositories) = fs::read_dir(&canonical_root) else {
        return Vec::new();
    };
    repositories
        .flatten()
        .filter_map(|repository| {
            let repository_path = repository.path();
            let metadata = fs::symlink_metadata(&repository_path).ok()?;
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || repository
                    .file_name()
                    .to_string_lossy()
                    .parse::<i64>()
                    .ok()?
                    <= 0
            {
                return None;
            }
            fs::read_dir(repository_path).ok()
        })
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name();
            if !has_generated_build_install_name(&name.to_string_lossy()) {
                return None;
            }
            let metadata = fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return None;
            }
            let canonical = fs::canonicalize(path).ok()?;
            if paths_equal(&canonical, &canonical_root)
                || !path_starts_with(&canonical, &canonical_root)
                || registered_paths
                    .iter()
                    .any(|registered| paths_equal(registered, &canonical))
                || canonical.join(emulator_binary_name()).exists()
            {
                return None;
            }
            Some(canonical)
        })
        .collect()
}
fn registered_build_paths(
    connection: &Connection,
    builds_root: &Path,
) -> Result<HashSet<PathBuf>, String> {
    let canonical_root = fs::canonicalize(builds_root).ok();
    let mut statement = connection
        .prepare("SELECT path FROM builds")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut paths = HashSet::new();
    for row in rows {
        if let Ok(path) = fs::canonicalize(row.map_err(|error| error.to_string())?) {
            if canonical_root
                .as_ref()
                .is_some_and(|root| path_starts_with(&path, root) && !paths_equal(&path, root))
            {
                paths.insert(path);
            }
        }
    }
    Ok(paths)
}
fn clean_incomplete_build_installations(
    builds_root: &Path,
    registered_paths: &HashSet<PathBuf>,
) -> Result<u64, String> {
    let mut removed = 0_u64;
    for path in incomplete_build_installations(builds_root, registered_paths) {
        let size = directory_size(&path);
        fs::remove_dir_all(&path)
            .map_err(|error| format!("Could not remove incomplete installation: {error}"))?;
        removed = removed.saturating_add(size);
    }
    Ok(removed)
}
#[tauri::command]
fn storage_report(app: tauri::AppHandle) -> Result<StorageReport, String> {
    let paths = state_paths(&app)?;
    let builds_root = paths.data.join("builds");
    let connection = db(&app)?;
    let registered = registered_build_paths(&connection, &builds_root)?;
    let installed_size = registered.iter().map(|path| directory_size(path)).sum();
    let incomplete = incomplete_build_installations(&builds_root, &registered);
    let incomplete_size = incomplete.iter().map(|path| directory_size(path)).sum();
    let mut categories = vec![
        StorageCategory {
            name: "Source repositories".into(),
            path: paths.data.join("sources").display().to_string(),
            size_bytes: directory_size(&paths.data.join("sources")),
        },
        StorageCategory {
            name: "Installed emulator builds".into(),
            path: builds_root.display().to_string(),
            size_bytes: installed_size,
        },
        StorageCategory {
            name: "Incomplete build installs".into(),
            path: builds_root.display().to_string(),
            size_bytes: incomplete_size,
        },
    ];
    categories.extend(
        [
            ("Compiler build cache", paths.cache.join("builds")),
            ("Session logs", paths.data.join("logs")),
            (
                "Community build worktrees",
                paths.cache.join("community-worktrees"),
            ),
        ]
        .into_iter()
        .map(|(name, path)| StorageCategory {
            name: name.into(),
            size_bytes: directory_size(&path),
            path: path.display().to_string(),
        }),
    );
    Ok(StorageReport { categories })
}
#[tauri::command]
fn clean_incomplete_builds(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
) -> Result<u64, String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if jobs.active.load(Ordering::SeqCst) {
        return Err("Wait for the active repository or build job to finish before cleaning incomplete installations".into());
    }
    let paths = state_paths(&app)?;
    let connection = db(&app)?;
    let registered = registered_build_paths(&connection, &paths.data.join("builds"))?;
    clean_incomplete_build_installations(&paths.data.join("builds"), &registered)
}
fn remove_managed_temp_directory(path: &Path, root: &Path) -> Result<u64, String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(0);
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Temporary build path is not a managed directory".into());
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let canonical_path = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if paths_equal(&canonical_path, &canonical_root)
        || !path_starts_with(&canonical_path, &canonical_root)
    {
        return Err("Temporary build path is outside Studio's managed cache".into());
    }
    let bytes = directory_size(&canonical_path);
    fs::remove_dir_all(&canonical_path)
        .map_err(|error| format!("Could not remove temporary build files: {error}"))?;
    Ok(bytes)
}
fn clear_managed_session_logs(
    connection: &mut Connection,
    logs_root: &Path,
    data_root: &Path,
) -> Result<u64, String> {
    let Ok(metadata) = fs::symlink_metadata(logs_root) else {
        return Ok(0);
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Session log path is not a managed directory".into());
    }
    let canonical_data = fs::canonicalize(data_root).map_err(|error| error.to_string())?;
    let canonical_logs = fs::canonicalize(logs_root).map_err(|error| error.to_string())?;
    if paths_equal(&canonical_logs, &canonical_data)
        || !path_starts_with(&canonical_logs, &canonical_data)
    {
        return Err("Session log path is outside Studio's managed data directory".into());
    }
    let bytes = directory_size(&canonical_logs);
    let mut statement = connection
        .prepare("SELECT id,log_path FROM sessions WHERE log_path<>''")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut session_ids = Vec::new();
    for row in rows {
        let (id, path) = row.map_err(|error| error.to_string())?;
        let path = PathBuf::from(path);
        let in_root = fs::canonicalize(&path)
            .is_ok_and(|canonical| path_starts_with(&canonical, &canonical_logs));
        if in_root {
            session_ids.push(id);
        }
    }
    drop(statement);

    let tombstone = canonical_data.join(format!(".session-logs-removing-{}", now()));
    if tombstone.exists() {
        return Err("A previous session log cleanup is still present".into());
    }
    fs::rename(&canonical_logs, &tombstone)
        .map_err(|error| format!("Could not stage session log cleanup: {error}"))?;
    if let Err(error) = fs::create_dir(&canonical_logs) {
        let _ = fs::rename(&tombstone, &canonical_logs);
        return Err(format!(
            "Could not recreate the session log directory: {error}"
        ));
    }
    let update_result = (|| {
        let tx = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        for id in &session_ids {
            tx.execute("UPDATE sessions SET log_path='' WHERE id=?", [id])
                .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())
    })();
    if let Err(error) = update_result {
        let _ = fs::remove_dir(&canonical_logs);
        let _ = fs::rename(&tombstone, &canonical_logs);
        return Err(error);
    }
    fs::remove_dir_all(&tombstone)
        .map_err(|error| format!("Session logs were unlinked, but staged files remain: {error}"))?;
    Ok(bytes)
}
struct ManagedTemporaryDirectory {
    path: PathBuf,
    root: PathBuf,
}
impl Drop for ManagedTemporaryDirectory {
    fn drop(&mut self) {
        let _ = remove_managed_temp_directory(&self.path, &self.root);
    }
}
#[tauri::command]
fn clean_compiler_cache(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
) -> Result<u64, String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if jobs.active.load(Ordering::SeqCst) {
        return Err("Wait for the active repository or build job to finish before cleaning the compiler cache".into());
    }
    let paths = state_paths(&app)?;
    let cache_root = fs::canonicalize(&paths.cache).map_err(|error| error.to_string())?;
    let data_root = fs::canonicalize(&paths.data).map_err(|error| error.to_string())?;
    let compiler_builds = cache_root.join("builds");
    let installed_builds = data_root.join("builds");
    if path_starts_with(&compiler_builds, &installed_builds)
        || path_starts_with(&installed_builds, &compiler_builds)
    {
        return Err(
            "Compiler cache overlaps installed emulator builds; cleanup was refused".into(),
        );
    }
    clear_managed_compiler_cache(&compiler_builds, &cache_root)
}
#[tauri::command]
fn clean_community_worktrees(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
) -> Result<u64, String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if jobs.active.load(Ordering::SeqCst) {
        return Err("Wait for the active repository or build job to finish before cleaning community worktrees".into());
    }
    let cache_root = state_paths(&app)?.cache;
    remove_managed_temp_directory(&cache_root.join("community-worktrees"), &cache_root)
}
#[tauri::command]
fn clean_session_logs(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
) -> Result<u64, String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if jobs.active.load(Ordering::SeqCst) {
        return Err(
            "Wait for active repository and build jobs to finish before clearing session logs"
                .into(),
        );
    }
    let _session_lifecycle = SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    let mut connection = db(&app)?;
    let active_sessions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_sessions > 0 {
        return Err("Stop active emulator sessions before clearing session logs".into());
    }
    let paths = state_paths(&app)?;
    clear_managed_session_logs(&mut connection, &paths.data.join("logs"), &paths.data)
}
fn clear_managed_compiler_cache(root: &Path, cache_root: &Path) -> Result<u64, String> {
    let Ok(metadata) = fs::symlink_metadata(root) else {
        return Ok(0);
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Compiler cache path is not a managed directory".into());
    }
    let canonical = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let canonical_cache_root = fs::canonicalize(cache_root).map_err(|error| error.to_string())?;
    if paths_equal(&canonical, &canonical_cache_root)
        || !path_starts_with(&canonical, &canonical_cache_root)
    {
        return Err("Compiler cache path is outside Studio's managed cache".into());
    }
    let bytes = directory_size(&canonical);
    fs::remove_dir_all(&canonical)
        .map_err(|error| format!("Could not clean compiler cache: {error}"))?;
    Ok(bytes)
}
fn db(app: &tauri::AppHandle) -> Result<Connection, String> {
    state_paths(app)?.db()
}
fn command_exists(name: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths).any(|directory| {
                if cfg!(windows) && Path::new(name).extension().is_none() {
                    directory.join(format!("{name}.exe")).is_file()
                } else {
                    directory.join(name).is_file()
                }
            })
        })
        .unwrap_or(false)
}
#[cfg(any(not(windows), test))]
fn any_command_available<F>(names: &[&str], mut available: F) -> bool
where
    F: FnMut(&str) -> bool,
{
    names.iter().any(|name| available(name))
}
fn graphics_diagnostics() -> GraphicsDiagnostics {
    let mut drivers = Vec::new();
    let mut monitors = Vec::new();
    #[cfg(not(windows))]
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(index) = name.strip_prefix("card") {
                if !index.is_empty() && index.chars().all(|c| c.is_ascii_digit()) {
                    let driver_path = path.join("device/driver");
                    if let Ok(driver_path) = fs::canonicalize(driver_path) {
                        if let Some(driver) = driver_path.file_name().and_then(|s| s.to_str()) {
                            drivers.push(format!("{name}: {driver}"));
                        }
                    }
                }
            }
            if name.contains('-')
                && fs::read_to_string(path.join("status"))
                    .is_ok_and(|status| status.trim() == "connected")
            {
                let modes = fs::read_to_string(path.join("modes")).unwrap_or_default();
                let modes = modes.lines().take(4).collect::<Vec<_>>().join(", ");
                let output_name = name
                    .split_once('-')
                    .map(|(_, output)| output)
                    .unwrap_or(&name);
                monitors.push(if modes.is_empty() {
                    output_name.to_string()
                } else {
                    format!("{output_name} · {modes}")
                });
            }
        }
    }
    #[cfg(windows)]
    let graphics_error = match windows_diagnostics::graphics() {
        Ok((windows_drivers, windows_monitors)) => {
            drivers = windows_drivers;
            monitors = windows_monitors;
            None
        }
        Err(error) => Some(error),
    };
    drivers.sort();
    drivers.dedup();
    monitors.sort();

    let (vulkan_status, vulkan_details) = if !command_exists("vulkaninfo") {
        (
            "Unavailable".into(),
            "Install vulkan-tools and a Vulkan ICD/driver package, then retry diagnostics.".into(),
        )
    } else {
        let mut command = host_tool_command("vulkaninfo");
        command
            .arg("--summary")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        prepare_owned_child(&mut command);
        let child = command
            .spawn()
            .map_err(|error| error.to_string())
            .and_then(|mut child| {
                attach_owned_child(&mut child)?;
                Ok(child)
            });
        match child {
            Err(error) => (
                "Failed".into(),
                format!("Could not safely start vulkaninfo: {error}"),
            ),
            Ok(mut child) => {
                let stdout = child.stdout.take().expect("stdout was piped");
                let stderr = child.stderr.take().expect("stderr was piped");
                let stdout_reader = thread::spawn(move || {
                    read_capped_output(BufReader::new(stdout), VULKANINFO_MAX_BYTES)
                });
                let stderr_reader = thread::spawn(move || {
                    read_capped_output(BufReader::new(stderr), VULKANINFO_MAX_BYTES)
                });
                let status =
                    wait_for_child_with_timeout(&mut child, Duration::from_secs(6), "vulkaninfo");
                let stdout = stdout_reader.join();
                let stderr = stderr_reader.join();
                match (status, stdout, stderr) {
                    (Err(error), _, _) if error.contains("time limit") => (
                        "Timed out".into(),
                        "vulkaninfo did not finish within six seconds; its process group was stopped.".into(),
                    ),
                    (Err(error), _, _) => (
                        "Failed".into(),
                        format!("Could not inspect vulkaninfo: {error}"),
                    ),
                    (Ok(status), Ok(Ok((stdout, stdout_truncated))), Ok(Ok((stderr, stderr_truncated)))) => {
                        let stdout = String::from_utf8_lossy(&stdout);
                        let lines = stdout
                            .lines()
                            .filter(|line| {
                                let line = line.trim_start();
                                line.starts_with("Vulkan Instance Version:")
                                    || line.starts_with("GPU")
                                    || line.starts_with("deviceName")
                                    || line.starts_with("driverName")
                                    || line.starts_with("driverInfo")
                                    || line.starts_with("apiVersion")
                            })
                            .take(40)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let stderr = String::from_utf8_lossy(&stderr);
                        let truncated = stdout_truncated || stderr_truncated;
                        let details = if !status.success() {
                            format!("{}\n{}", lines, stderr.lines().take(8).collect::<Vec<_>>().join("\n"))
                        } else if truncated {
                            format!("{lines}\n[vulkaninfo output truncated for safety]")
                        } else {
                            lines
                        };
                        if !status.success() {
                            ("Failed".into(), details)
                        } else if !stdout.contains("GPU") && !stdout.contains("deviceName") {
                            ("No device found".into(), "vulkaninfo ran, but no Vulkan physical device was reported. Check the installed Vulkan ICD and active GPU driver.".into())
                        } else {
                            ("Available".into(), details)
                        }
                    }
                    (_, Ok(Err(error)), _) | (_, _, Ok(Err(error))) => (
                        "Failed".into(),
                        format!("Could not read vulkaninfo output: {error}"),
                    ),
                    (_, Err(_), _) | (_, _, Err(_)) => (
                        "Failed".into(),
                        "Could not collect vulkaninfo output: reader thread failed".into(),
                    ),
                }
            }
        }
    };
    #[cfg(windows)]
    let vulkan_details = graphics_error.map_or(vulkan_details.clone(), |error| {
        format!("{vulkan_details}\n\nWindows display diagnostics: {error}")
    });
    GraphicsDiagnostics {
        vulkan_status,
        vulkan_details,
        drivers,
        monitors,
    }
}
#[cfg(any(not(windows), test))]
fn memory_value_bytes(meminfo: &str, key: &str) -> Option<u64> {
    meminfo.lines().find_map(|line| {
        let (name, rest) = line.split_once(':')?;
        if name != key {
            return None;
        }
        rest.split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    })
}
#[cfg(any(not(windows), test))]
fn detected_controllers(input_devices: &str) -> Vec<String> {
    let mut found = Vec::new();
    for block in input_devices.split("\n\n") {
        let name = block
            .lines()
            .find_map(|line| line.strip_prefix("N: Name="))
            .unwrap_or("")
            .trim_matches('"');
        if name.is_empty() {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if [
            "keyboard",
            "mouse",
            "touchpad",
            "touchscreen",
            "touch screen",
            "trackpoint",
        ]
        .iter()
        .any(|kind| lower.contains(kind))
        {
            continue;
        }
        let handlers = block
            .lines()
            .find(|line| line.starts_with("H: Handlers="))
            .unwrap_or("");
        let kind = if lower.contains("dualsense") {
            Some("DualSense")
        } else if lower.contains("dualshock")
            || (lower.contains("sony") && lower.contains("wireless controller"))
        {
            Some("DualShock 4 / PlayStation")
        } else if lower.contains("xbox") || lower.contains("x-box") {
            Some("Xbox")
        } else if handlers
            .split_whitespace()
            .any(|handler| handler.starts_with("js"))
            || (block.lines().any(|line| {
                line.strip_prefix("B: ABS=")
                    .is_some_and(|bits| bits.chars().any(|bit| bit != '0' && !bit.is_whitespace()))
            }) && block.lines().any(|line| {
                line.strip_prefix("B: KEY=")
                    .is_some_and(|bits| bits.chars().any(|bit| bit != '0' && !bit.is_whitespace()))
            }))
        {
            Some("Generic controller")
        } else {
            None
        };
        if let Some(kind) = kind {
            let label = format!("{kind}: {name}");
            if !found.contains(&label) {
                found.push(label);
            }
        }
    }
    found
}
fn host_tool_command(program: &str) -> Command {
    #[cfg(target_os = "linux")]
    let mut command = if Path::new("/usr/bin").join(program).is_file() {
        Command::new(Path::new("/usr/bin").join(program))
    } else {
        Command::new(program)
    };
    #[cfg(not(target_os = "linux"))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(target_os = "linux")]
    {
        let mut search_path = vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")];
        if let Some(path) = env::var_os("PATH") {
            search_path.extend(env::split_paths(&path));
        }
        if let Ok(path) = env::join_paths(search_path) {
            command.env("PATH", path);
        }
        // AppImage launchers add private libraries to LD_LIBRARY_PATH. Host tools
        // such as CMake and Git must load their matching system libraries instead.
        // The AppImage Python runtime also sets PYTHONHOME/PYTHONPATH; inheriting
        // those values makes host Python use the bundled stdlib and CMake's
        // FindPython3 check fail before configuration can finish.
        command.env_remove("LD_LIBRARY_PATH");
        command.env_remove("LD_PRELOAD");
        command.env_remove("PYTHONHOME");
        command.env_remove("PYTHONPATH");
    }
    command
}

fn git_command() -> Command {
    #[cfg(target_os = "linux")]
    let mut command = host_tool_command("git");
    #[cfg(not(target_os = "linux"))]
    let command = host_tool_command("git");
    #[cfg(target_os = "linux")]
    {
        command.env_remove("GIT_EXEC_PATH");
    }
    command
}

fn git_path_arg(path: &Path) -> Result<String, String> {
    let path = path.to_string_lossy();
    #[cfg(windows)]
    {
        let ordinary = path
            .strip_prefix(r"\\?\UNC\")
            .map(|unc| format!(r"\\{unc}"))
            .or_else(|| path.strip_prefix(r"\\?\").map(str::to_owned))
            .unwrap_or_else(|| path.into_owned());
        if ordinary.encode_utf16().count() >= 260 {
            Err("The Community PR build path is too long for Git on Windows; use a shorter source or Studio data path".into())
        } else {
            Ok(ordinary)
        }
    }
    #[cfg(not(windows))]
    Ok(path.into_owned())
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut c = git_command();
    if let Some(d) = dir {
        c.arg("-C").arg(d);
    }
    prepare_owned_child(&mut c);
    let mut child = c
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| error.to_string())?;
    attach_owned_child(&mut child)?;
    let stdout = child.stdout.take().ok_or("Git stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("Git stderr unavailable")?;
    let stdout_reader =
        thread::spawn(move || read_capped_output(BufReader::new(stdout), GIT_CAPTURE_MAX_BYTES));
    let stderr_reader =
        thread::spawn(move || read_capped_output(BufReader::new(stderr), GIT_CAPTURE_MAX_BYTES));
    let wait_result =
        wait_for_child_with_timeout(&mut child, Duration::from_secs(120), "Git command");
    let status = wait_result.map_err(|error| format!("Could not wait for Git: {error}"))?;
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "Git output reader failed")?
        .map_err(|error| format!("Could not read Git output: {error}"))?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| "Git error reader failed")?
        .map_err(|error| format!("Could not read Git error output: {error}"))?;
    if stdout_truncated || stderr_truncated {
        return Err(format!(
            "Git command output exceeded the {} MiB safety limit",
            GIT_CAPTURE_MAX_BYTES / (1024 * 1024)
        ));
    }
    if !status.success() {
        return Err(String::from_utf8_lossy(&stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&stdout).trim().to_string())
}
#[cfg(target_os = "linux")]
fn wait_for_child_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
    description: &str,
) -> Result<std::process::ExitStatus, String> {
    let pid = child.id() as i32;
    let deadline = Instant::now() + timeout;
    loop {
        match child_exit_observed(pid as u32) {
            Ok(true) => {
                let cleanup = stop_process_group(pid);
                let status = child
                    .wait()
                    .map_err(|error| format!("Could not reap {description}: {error}"))?;
                cleanup?;
                return Ok(status);
            }
            Ok(false) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(false) => {
                let cleanup = stop_process_group(pid);
                let _ = child.wait();
                return Err(format!(
                    "{description} exceeded its {} second time limit{}",
                    timeout.as_secs(),
                    cleanup
                        .err()
                        .map(|error| format!("; process cleanup failed: {error}"))
                        .unwrap_or_default()
                ));
            }
            Err(error) => {
                let cleanup = stop_process_group(pid);
                let _ = child.wait();
                return Err(format!(
                    "Could not wait for {description}: {error}{}",
                    cleanup
                        .err()
                        .map(|error| format!("; process cleanup failed: {error}"))
                        .unwrap_or_default()
                ));
            }
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn wait_for_child_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
    description: &str,
) -> Result<std::process::ExitStatus, String> {
    let pid = child.id() as i32;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let cleanup = stop_process_group(pid);
                #[cfg(windows)]
                release_owned_child(pid);
                cleanup?;
                return Ok(status);
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let cleanup = stop_process_group(pid);
                #[cfg(windows)]
                release_owned_child(pid);
                #[cfg(windows)]
                let wait = wait_for_child_exit_bounded(child, Duration::from_secs(3), description);
                #[cfg(not(windows))]
                let wait = child.wait().map_err(|error| error.to_string());
                return Err(format!(
                    "{description} exceeded its {} second time limit{}{}",
                    timeout.as_secs(),
                    cleanup
                        .err()
                        .map(|error| format!("; process cleanup failed: {error}"))
                        .unwrap_or_default(),
                    wait.err()
                        .map(|error| format!("; bounded reap failed: {error}"))
                        .unwrap_or_default()
                ));
            }
            Err(error) => {
                let cleanup = stop_process_group(pid);
                #[cfg(windows)]
                release_owned_child(pid);
                #[cfg(windows)]
                let wait = wait_for_child_exit_bounded(child, Duration::from_secs(3), description);
                #[cfg(not(windows))]
                let wait = child.wait().map_err(|error| error.to_string());
                return Err(format!(
                    "Could not wait for {description}: {error}{}{}",
                    cleanup
                        .err()
                        .map(|error| format!("; process cleanup failed: {error}"))
                        .unwrap_or_default(),
                    wait.err()
                        .map(|error| format!("; bounded reap failed: {error}"))
                        .unwrap_or_default()
                ));
            }
        }
    }
}
#[cfg(windows)]
fn wait_for_child_exit_bounded(
    child: &mut std::process::Child,
    timeout: Duration,
    description: &str,
) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                return Err(format!(
                    "{description} remained alive after the {} second cleanup window",
                    timeout.as_secs()
                ));
            }
            Err(error) => return Err(format!("Could not reap {description}: {error}")),
        }
    }
}
const GIT_CAPTURE_MAX_BYTES: usize = 8 * 1024 * 1024;
const VULKANINFO_MAX_BYTES: usize = 1024 * 1024;
const EMULATOR_HELP_MAX_BYTES: usize = 1024 * 1024;
fn read_capped_output<R: Read>(mut reader: R, maximum: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::with_capacity(maximum.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = maximum.saturating_sub(output.len());
        let retained = count.min(remaining);
        output.extend_from_slice(&buffer[..retained]);
        truncated |= retained < count;
    }
    Ok((output, truncated))
}
fn git_cancellable(
    app: &tauri::AppHandle,
    jobs: &Jobs,
    dir: Option<&Path>,
    args: &[&str],
) -> Result<String, String> {
    git_cancellable_env(app, jobs, dir, args, &[])
}
fn git_cancellable_env(
    app: &tauri::AppHandle,
    jobs: &Jobs,
    dir: Option<&Path>,
    args: &[&str],
    environment: &[(&str, &str)],
) -> Result<String, String> {
    ensure_job_not_cancelled(jobs)?;
    let mut command = git_command();
    if let Some(directory) = dir {
        command.arg("-C").arg(directory);
    }
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    for (key, value) in environment {
        command.env(key, value);
    }
    prepare_owned_child(&mut command);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    attach_owned_child(&mut child)?;
    let pid = child.id() as i32;
    jobs.pid.store(pid, Ordering::SeqCst);
    let stdout = child.stdout.take().ok_or("Git stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("Git stderr unavailable")?;
    let stdout_reader =
        thread::spawn(move || read_capped_output(BufReader::new(stdout), GIT_CAPTURE_MAX_BYTES));
    let stderr_reader =
        thread::spawn(move || read_capped_output(BufReader::new(stderr), GIT_CAPTURE_MAX_BYTES));
    let mut cancellation_sent = false;
    let observed = loop {
        if jobs.cancel_requested.load(Ordering::SeqCst) && !cancellation_sent {
            cancellation_sent = true;
            let _ = stop_process_group(pid);
        }
        match child.try_wait() {
            Ok(Some(_)) => break Ok(()),
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(error) => break Err(format!("Could not wait for Git: {error}")),
        }
    };
    let cleanup_result = stop_process_group(pid);
    // Keep the unreaped child as the owner of its PID until its process group
    // has been cleaned, then clear the cancellation target before reaping it.
    jobs.pid.store(0, Ordering::SeqCst);
    #[cfg(windows)]
    release_owned_child(pid);
    #[cfg(windows)]
    let wait_result = wait_for_child_exit_bounded(&mut child, Duration::from_secs(3), "Git");
    #[cfg(not(windows))]
    let wait_result = child
        .wait()
        .map_err(|error| format!("Could not reap Git: {error}"));
    let status = match wait_result {
        Ok(status) => status,
        Err(error) => {
            return Err(format!(
                "Could not reap Git: {error}{}{}",
                cleanup_result
                    .err()
                    .map(|error| format!("; process cleanup failed: {error}"))
                    .unwrap_or_default(),
                observed
                    .err()
                    .map(|error| format!("; process wait failed: {error}"))
                    .unwrap_or_default()
            ));
        }
    };
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "Git output reader failed")?
        .map_err(|error| format!("Could not read Git output: {error}"))?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| "Git error reader failed")?
        .map_err(|error| format!("Could not read Git error output: {error}"))?;
    cleanup_result?;
    observed?;
    if stdout_truncated || stderr_truncated {
        return Err(format!(
            "Git command output exceeded the {} MiB safety limit",
            GIT_CAPTURE_MAX_BYTES / (1024 * 1024)
        ));
    }
    let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
    if jobs.cancel_requested.load(Ordering::SeqCst) {
        return Err("Job cancelled".into());
    }
    if !status.success() {
        return Err(if stderr.is_empty() {
            "Git command failed".into()
        } else {
            stderr
        });
    }
    let event_kind = jobs
        .event_kind
        .lock()
        .map(|kind| kind.clone())
        .unwrap_or_else(|_| "build".into());
    if !stdout.is_empty() {
        emit(app, &event_kind, "Git", stdout.clone(), false, true);
    }
    if !stderr.is_empty() {
        emit(app, &event_kind, "Git", stderr, false, true);
    }
    Ok(stdout)
}
fn git_deterministic_merge(
    app: &tauri::AppHandle,
    jobs: &Jobs,
    dir: &Path,
    reference: &str,
) -> Result<String, String> {
    git_cancellable_env(
        app,
        jobs,
        Some(dir),
        &["merge", "--no-edit", "--no-ff", reference],
        &[
            ("GIT_AUTHOR_DATE", "1970-01-01T00:00:00Z"),
            ("GIT_COMMITTER_DATE", "1970-01-01T00:00:00Z"),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "commit.gpgsign"),
            ("GIT_CONFIG_VALUE_0", "false"),
        ],
    )
}
#[cfg(test)]
fn git_deterministic_merge_test(dir: &Path, reference: &str) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["merge", "--no-edit", "--no-ff", reference])
        .env("GIT_AUTHOR_DATE", "1970-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "1970-01-01T00:00:00Z")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "commit.gpgsign")
        .env("GIT_CONFIG_VALUE_0", "false")
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
#[allow(clippy::too_many_arguments)] // Keep each signature input explicit in the cache key.
fn community_build_signature(
    repository_id: i64,
    base_branch: &str,
    base_sha: &str,
    pull_requests: &[(u64, String)],
    build_type: &str,
    jobs: u16,
    compiler: &str,
    clean: bool,
) -> String {
    let ordered_heads = pull_requests
        .iter()
        .map(|(number, sha)| format!("{number}-{sha}"))
        .collect::<Vec<_>>()
        .join("-");
    format!(
        "repo-{repository_id}-{base_branch}-{base_sha}-{ordered_heads}-{build_type}-{jobs}-{compiler}-clean-{clean}"
    )
}
fn checked_out_branch(path: &Path) -> Result<String, String> {
    let branch = git(Some(path), &["branch", "--show-current"])?;
    if branch.is_empty() || branch == "HEAD" {
        return Err("Repository is not on a named branch".into());
    }
    Ok(branch)
}
fn parse_repo(input: &str) -> Result<(String, String, String), String> {
    let trimmed = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let url =
        url::Url::parse(trimmed).map_err(|_| "Use a GitHub HTTPS repository URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Only GitHub HTTPS repository URLs are accepted".into());
    }
    let parts: Vec<_> = url
        .path_segments()
        .ok_or("Invalid repository path")?
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() != 2
        || parts.iter().any(|s| {
            !s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err("Expected https://github.com/owner/repository".into());
    }
    Ok((
        parts[0].into(),
        parts[1].into(),
        format!("https://github.com/{}/{}", parts[0], parts[1]),
    ))
}
fn valid_branch_name(branch: &str) -> bool {
    if branch.is_empty() {
        return false;
    }
    git_command()
        .args(["check-ref-format", "--branch", branch])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
fn repo(conn: &Connection, id: i64) -> Result<Repo, String> {
    conn.query_row("SELECT id,owner,name,url,branch,source_path,adapter,installed_commit,removed_at IS NOT NULL FROM repositories WHERE id=?",[id],|r|Ok(Repo{id:r.get(0)?,owner:r.get(1)?,name:r.get(2)?,url:r.get(3)?,branch:r.get(4)?,source_path:r.get(5)?,adapter:r.get(6)?,installed_commit:r.get(7)?,removed:r.get::<_,i64>(8)?!=0})).map_err(|e|e.to_string())
}
#[cfg(windows)]
fn windows_explorer_path(system_root: &std::ffi::OsStr) -> PathBuf {
    PathBuf::from(system_root).join("explorer.exe")
}
fn open_directory(path: &Path) -> Result<(), String> {
    let directory =
        fs::canonicalize(path).map_err(|error| format!("Folder is unavailable: {error}"))?;
    if !directory.is_dir() {
        return Err("Selected location is not a directory".into());
    }
    #[cfg(unix)]
    let mut child = host_tool_command("xdg-open")
        .arg(directory)
        .spawn()
        .map_err(|error| {
            format!("Could not open the folder with the desktop file manager: {error}")
        })?;
    #[cfg(windows)]
    let mut child = Command::new({
        let system_root = env::var_os("SystemRoot")
            .filter(|root| !root.is_empty())
            .ok_or("Windows SystemRoot is unavailable")?;
        let explorer = windows_explorer_path(&system_root);
        if !explorer.is_file() {
            return Err("Windows Explorer is unavailable under SystemRoot".into());
        }
        explorer
    })
    .arg(directory)
    .spawn()
    .map_err(|error| format!("Could not open the folder with the desktop file manager: {error}"))?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
#[tauri::command]
fn open_repository_folder(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let c = db(&app)?;
    let repository = repo(&c, id)?;
    let source = repository.source_path.ok_or("Clone the repository first")?;
    open_directory(Path::new(&source))
}
#[tauri::command]
fn open_build_folder(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let c = db(&app)?;
    let installation = build(&c, id)?;
    open_directory(Path::new(&installation.path))
}
#[tauri::command]
fn open_game_folder(app: tauri::AppHandle, game_id: i64) -> Result<(), String> {
    let c = db(&app)?;
    let game = game(&c, game_id)?;
    let path = Path::new(&game.path);
    let directory = if path.is_dir() {
        path
    } else {
        path.parent().ok_or("Game path has no parent directory")?
    };
    open_directory(directory)
}
fn shader_precompile_files(build_path: &Path) -> Option<(PathBuf, PathBuf)> {
    let script = precompile_script_candidates(build_path)
        .into_iter()
        .find(|path| path.is_file())?;
    let script_requires_windows_tool = precompile_script_requires_windows_tool(&script);
    precompile_tool_candidates(build_path)
        .iter()
        .find(|tool| {
            if script_requires_windows_tool {
                tool.file_name()
                    .is_some_and(|name| name == "kyty_shader_precompile.exe")
                    && tool.is_file()
            } else {
                shader_precompile_tool_available(tool)
            }
        })
        .cloned()
        .map(|tool| (script, tool))
}
fn precompile_script_candidates(root: &Path) -> Vec<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &[
            "precompile-windows.ps1",
            "precompile.ps1",
            "precompile.cmd",
            "precompile.bat",
        ]
    } else {
        &["precompile-linux.sh", "precompile.sh"]
    };
    names.iter().map(|name| root.join(name)).collect()
}
fn precompile_script_requires_windows_tool(script: &Path) -> bool {
    fs::read_to_string(script).is_ok_and(|contents| contents.contains("kyty_shader_precompile.exe"))
}
fn configured_build_directory(build: &Build) -> Option<PathBuf> {
    let args = &build.configuration.cmake_arguments;
    args.iter().enumerate().find_map(|(index, argument)| {
        if argument == "-B" {
            args.get(index + 1).map(PathBuf::from)
        } else {
            argument
                .strip_prefix("-B")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        }
    })
}
fn stage_precompile_for_build(build: &Build, repository: &Repo) -> Result<(), String> {
    if shader_precompile_files(Path::new(&build.path)).is_some() {
        return Ok(());
    }
    let Some(source) = repository.source_path.as_deref().map(Path::new) else {
        return Ok(());
    };
    let source_commit = git(Some(source), &["rev-parse", "HEAD"]).ok();
    let source_changes = git(Some(source), &["status", "--porcelain"]).ok();
    if source_commit.as_deref() != Some(build.commit_sha.as_str())
        || source_changes.as_deref() != Some("")
    {
        return Ok(());
    }
    let build_dir = configured_build_directory(build).unwrap_or_else(|| PathBuf::from(&build.path));
    stage_shader_precompile_assets(source, &build_dir, Path::new(&build.path))
}
#[tauri::command]
fn shader_precompile_support(
    app: tauri::AppHandle,
    game_id: i64,
    build_id: i64,
) -> Result<ShaderPrecompileSupport, String> {
    let connection = db(&app)?;
    let installation = build(&connection, build_id)?;
    let repository = repo(&connection, installation.repo_id)?;
    stage_precompile_for_build(&installation, &repository)?;
    let background_precompile = source_proves_background_precompile(&installation, &repository);
    Ok(ShaderPrecompileSupport {
        full_precompile: shader_precompile_files(Path::new(&installation.path)).is_some(),
        background_precompile,
        background_enabled: background_precompile
            && get_game_shader_precompile_enabled(&connection, game_id, build_id)?,
        cache_directory: Path::new(&installation.path)
            .join("_PipelineCache")
            .display()
            .to_string(),
    })
}
#[tauri::command]
async fn precompile_game_shaders(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    game_id: i64,
    build_id: i64,
) -> Result<(), String> {
    let worker_app = app.clone();
    let worker_jobs = jobs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        precompile_game_shaders_sync(worker_app, worker_jobs, game_id, build_id)
    })
    .await
    .map_err(|error| format!("Shader precompile task failed: {error}"))?
}
fn precompile_game_shaders_sync(
    app: tauri::AppHandle,
    jobs: Arc<Jobs>,
    game_id: i64,
    build_id: i64,
) -> Result<(), String> {
    begin_job(
        &jobs,
        "A build, update, or shader precompile job is already running",
        true,
        "shader-precompile",
    )?;
    SHADER_PRECOMPILE_ACTIVE.store(true, Ordering::Release);
    let result = (|| {
        let (game_info, installation, repository) = {
            let _session_lifecycle = SESSION_LIFECYCLE_LOCK
                .lock()
                .map_err(|error| error.to_string())?;
            let c = db(&app)?;
            let game_info = game(&c, game_id)?;
            let installation = build(&c, build_id)?;
            let repository = repo(&c, installation.repo_id)?;
            let active_session: bool = c
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sessions WHERE ended_at IS NULL)",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if active_session {
                return Err("Close the running emulator before precompiling shaders".into());
            }
            (game_info, installation, repository)
        };
        stage_precompile_for_build(&installation, &repository)?;
        let (script, _tool) = shader_precompile_files(Path::new(&installation.path))
            .ok_or("The selected build does not include a full shader precompile tool")?;
        let mut command = if cfg!(windows) {
            if script
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd"))
            {
                let mut command = Command::new("cmd.exe");
                command
                    .arg("/C")
                    .arg(script)
                    .arg("-Game")
                    .arg(&game_info.path);
                command
            } else {
                let mut command = Command::new("powershell.exe");
                command
                    .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                    .arg(script)
                    .args(["-Game"])
                    .arg(&game_info.path);
                command
            }
        } else {
            let mut command = Command::new("bash");
            command.arg(script).args(["--game"]).arg(&game_info.path);
            command
        };
        command
            .current_dir(&installation.path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        prepare_owned_child(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not start the shader precompile tool: {error}"))?;
        attach_owned_child(&mut child)?;
        let pid = child.id() as i32;
        jobs.pid.store(pid, Ordering::SeqCst);
        if jobs.cancel_requested.load(Ordering::SeqCst) {
            let _ = stop_process_group(pid);
        }
        emit(
            &app,
            "shader-precompile",
            "Precompiling",
            format!("Precompiling shaders for {}", game_info.title),
            false,
            true,
        );
        emit_shader_progress(
            &app,
            ShaderPrecompileProgress {
                shaders_total: None,
                pipelines_compiled: 0,
                pipelines_total: None,
                live_workers: 0,
            },
        );
        let installation_root = PathBuf::from(&installation.path);
        let log_directory = Arc::new(Mutex::new(None::<PathBuf>));
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_app = app.clone();
        let stdout_root = installation_root.clone();
        let stdout_log_directory = log_directory.clone();
        let stdout_reader = thread::spawn(move || {
            if let Some(stdout) = stdout {
                drain_shader_precompile_output(
                    stdout,
                    "stdout",
                    &stdout_app,
                    &stdout_root,
                    &stdout_log_directory,
                );
            }
        });
        let stderr_app = app.clone();
        let stderr_root = installation_root.clone();
        let stderr_log_directory = log_directory.clone();
        let stderr_reader = thread::spawn(move || {
            if let Some(stderr) = stderr {
                drain_shader_precompile_output(
                    stderr,
                    "stderr",
                    &stderr_app,
                    &stderr_root,
                    &stderr_log_directory,
                );
            }
        });
        let wait_jobs = jobs.clone();
        let wait_thread = thread::spawn(move || {
            wait_for_child_and_cleanup_group(&mut child, || {
                wait_jobs.pid.store(0, Ordering::SeqCst);
            })
        });
        let mut previous_progress = None;
        while !wait_thread.is_finished() {
            let directory = log_directory
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone();
            if let Some(directory) = directory {
                let progress = read_shader_precompile_progress(&directory);
                if previous_progress.as_ref() != Some(&progress) {
                    emit_shader_progress(&app, progress.clone());
                    previous_progress = Some(progress);
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
        let waited = wait_thread
            .join()
            .map_err(|_| "Shader precompile process waiter panicked".to_string())??;
        let (status, cleanup) = waited;
        cleanup?;
        if let Some(directory) = log_directory
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
        {
            emit_shader_progress(&app, read_shader_precompile_progress(&directory));
        }
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        if !status.success() {
            return Err(format!("Shader precompile exited with status {status}"));
        }
        Ok(())
    })();
    SHADER_PRECOMPILE_ACTIVE.store(false, Ordering::Release);
    match result {
        Ok(()) => {
            finish_job(
                &app,
                &jobs,
                "shader-precompile",
                Ok("Shader precompile completed".into()),
            );
            Ok(())
        }
        Err(error) => {
            finish_job(&app, &jobs, "shader-precompile", Err(error.clone()));
            Err(error)
        }
    }
}

fn emit_shader_progress(app: &tauri::AppHandle, progress: ShaderPrecompileProgress) {
    let _ = app.emit(
        "studio-job",
        JobEvent {
            job_id: ACTIVE_JOB_ID.load(Ordering::SeqCst),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            channel: "shader-precompile".into(),
            kind: "shader-precompile".into(),
            phase: "Precompiling".into(),
            line: "Shader compilation progress".into(),
            done: false,
            success: true,
            progress: Some(progress),
        },
    );
}

fn drain_shader_precompile_output<R: Read>(
    stream: R,
    label: &str,
    app: &tauri::AppHandle,
    installation_root: &Path,
    log_directory: &Arc<Mutex<Option<PathBuf>>>,
) {
    let mut reader = CappedOutputReader::new(stream);
    loop {
        match reader.next_line() {
            Ok(Some(output)) => {
                let line = output.text();
                if let Some((_, path)) = line.rsplit_once("; logs ") {
                    let path = PathBuf::from(path.trim());
                    let path = if path.is_absolute() {
                        path
                    } else {
                        installation_root.join(path)
                    };
                    *log_directory
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(path);
                }
                emit(app, "shader-precompile", "Precompiling", line, false, true);
            }
            Ok(None) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                emit(
                    app,
                    "shader-precompile",
                    "Precompiling",
                    format!("{label} output reader error: {error}"),
                    false,
                    false,
                );
                break;
            }
        }
    }
}

fn read_shader_precompile_progress(log_directory: &Path) -> ShaderPrecompileProgress {
    let mut progress = ShaderPrecompileProgress {
        shaders_total: None,
        pipelines_compiled: 0,
        pipelines_total: None,
        live_workers: 0,
    };
    let mut total_pipelines = 0_u64;
    let mut saw_plan = false;
    for plan_name in ["plan-recorded.out.log", "plan-seeds.out.log"] {
        let path = log_directory.join(plan_name);
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for line in contents.lines() {
            if line.contains(" of them this seed file's") {
                if let Some((_, count)) = line.split_once(" pipelines,") {
                    if let Some(value) = count
                        .split_whitespace()
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                    {
                        total_pipelines = total_pipelines.saturating_add(value);
                        saw_plan = true;
                    }
                }
            }
            if plan_name == "plan-seeds.out.log" {
                if let Some(value) = line
                    .strip_prefix("Shader seeds: ")
                    .and_then(|line| line.split_once(" programs"))
                    .and_then(|(value, _)| value.parse::<u64>().ok())
                {
                    progress.shaders_total = Some(value);
                }
            }
        }
    }
    if saw_plan {
        progress.pipelines_total = Some(total_pipelines);
    }

    let Ok(entries) = fs::read_dir(log_directory) else {
        return progress;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        if !name.starts_with("shard") || !name.ends_with(".out.log") {
            continue;
        }
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        if !contents.contains("Shader precompile: complete") {
            progress.live_workers = progress.live_workers.saturating_add(1);
            continue;
        }
        for line in contents.lines() {
            if let Some((_, saved)) = line.split_once("Static pipeline binaries: saved ") {
                if let Some(value) = saved
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                {
                    progress.pipelines_compiled = progress.pipelines_compiled.saturating_add(value);
                }
            }
        }
    }
    progress
}
fn build(conn: &Connection, id: i64) -> Result<Build, String> {
    let mut result = conn.query_row(
        "SELECT id,repo_id,commit_sha,build_type,path,created_at,active,install_size_bytes,validation_status,
         (SELECT started_at FROM sessions WHERE build_id=builds.id ORDER BY started_at DESC,id DESC LIMIT 1),
         (SELECT outcome FROM sessions WHERE build_id=builds.id ORDER BY started_at DESC,id DESC LIMIT 1),
         (SELECT exit_code FROM sessions WHERE build_id=builds.id ORDER BY started_at DESC,id DESC LIMIT 1),
         configuration_json
         FROM builds WHERE id=?",
        [id],
        |r| {
            Ok(Build {
                id: r.get(0)?,
                repo_id: r.get(1)?,
                commit_sha: r.get(2)?,
                build_type: r.get(3)?,
                path: {
                    let path: String = r.get(4)?;
                    path
                },
                created_at: r.get(5)?,
                active: r.get::<_, i64>(6)? != 0,
                executable_status: {
                    let executable = emulator_executable(Path::new(&r.get::<_, String>(4)?));
                    match fs::metadata(&executable) {
                        Ok(metadata) if metadata.is_file() && is_emulator_executable(&executable) => "Executable present".to_string(),
                        Ok(metadata) if metadata.is_file() => "Not executable".to_string(),
                        _ => "Missing".to_string(),
                    }
                },
                install_size_bytes: r.get::<_, Option<i64>>(7)?.and_then(|size| u64::try_from(size).ok()),
                validation_status: r.get(8)?,
                last_session_at: r.get(9)?,
                last_session_outcome: r.get(10)?,
                last_session_exit_code: r.get(11)?,
                configuration: serde_json::from_str(&r.get::<_, String>(12)?)
                    .unwrap_or_default(),
                community_prs: Vec::new(),
            })
        },
    )
    .map_err(|e| e.to_string())?;
    let mut statement = conn.prepare(
        "SELECT repositories.owner,repositories.name,community_build_prs.number,community_build_prs.head_sha
         FROM community_build_results
         JOIN community_build_configs ON community_build_configs.id=community_build_results.config_id
         JOIN repositories ON repositories.id=community_build_configs.repo_id
         JOIN community_build_prs ON community_build_prs.config_id=community_build_configs.id
         WHERE community_build_results.build_id=? AND community_build_results.status='verified'
         ORDER BY community_build_results.started_at,community_build_prs.position",
    ).map_err(|e| e.to_string())?;
    result.community_prs = statement
        .query_map([id], |row| {
            let owner: String = row.get(0)?;
            let name: String = row.get(1)?;
            let number: i64 = row.get(2)?;
            Ok(BuildPullRequest {
                number: number.max(0) as u64,
                head_sha: row.get(3)?,
                url: format!("https://github.com/{owner}/{name}/pull/{number}"),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    result
        .community_prs
        .dedup_by(|a, b| a.number == b.number && a.head_sha == b.head_sha);
    Ok(result)
}
fn game(conn: &Connection, id: i64) -> Result<Game, String> {
    conn.query_row(
        "SELECT id,path,title,favorite,build_id,play_seconds,last_played,title_id,game_version,icon_path,install_size FROM games WHERE id=?",
        [id],
        |r| {
            Ok(Game {
                id: r.get(0)?,
                path: r.get(1)?,
                title: r.get(2)?,
                favorite: r.get::<_, i64>(3)? != 0,
                build_id: r.get(4)?,
                play_seconds: r.get(5)?,
                last_played: r.get(6)?,
                title_id: r.get(7)?,
                game_version: r.get(8)?,
                icon_path: r.get(9)?,
                install_size: r.get(10)?,
                file_status: game_file_status(Path::new(&r.get::<_, String>(1)?)).into(),
            })
        },
    )
    .map_err(|e| e.to_string())
}
fn game_file_status(path: &Path) -> &'static str {
    if fs::metadata(path).is_ok() {
        "Available"
    } else {
        "Missing"
    }
}
#[tauri::command]
fn list_repos(app: tauri::AppHandle) -> Result<Vec<Repo>, String> {
    let c = db(&app)?;
    let mut s = c
        .prepare("SELECT id FROM repositories ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for id in rows {
        let id = id.map_err(|e| e.to_string())?;
        let repository = repo(&c, id)?;
        if repository
            .source_path
            .as_deref()
            .is_some_and(|path| !Path::new(path).is_dir())
        {
            c.execute(
                "UPDATE repositories SET source_path=NULL,adapter='unknown' WHERE id=?",
                [id],
            )
            .map_err(|error| error.to_string())?;
        }
        result.push(repo(&c, id)?);
    }
    Ok(result)
}
#[tauri::command]
fn list_builds(app: tauri::AppHandle) -> Result<Vec<Build>, String> {
    let mut c = db(&app)?;
    let mut s = c
        .prepare("SELECT id FROM builds WHERE removed_at IS NULL ORDER BY created_at DESC,id DESC")
        .map_err(|e| e.to_string())?;
    let rows = s
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(|e| e.to_string())?;
    drop(s);
    for id in rows {
        let installed = build(&c, id)?;
        if !Path::new(&installed.path).is_dir() {
            retire_missing_build(&mut c, id)?;
        }
    }
    let mut s = c
        .prepare("SELECT id FROM builds WHERE removed_at IS NULL ORDER BY created_at DESC,id DESC")
        .map_err(|e| e.to_string())?;
    let rows = s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
    rows.map(|id| build(&c, id.map_err(|e| e.to_string())?))
        .collect()
}
#[tauri::command]
fn list_games(app: tauri::AppHandle) -> Result<Vec<Game>, String> {
    let c = db(&app)?;
    let mut s = c
        .prepare("SELECT id,path,title,favorite,build_id,play_seconds,last_played,title_id,game_version,icon_path,install_size FROM games WHERE hidden=0 ORDER BY favorite DESC, title COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = s
        .query_map([], |r| {
            let path: String = r.get(1)?;
            Ok(Game {
                id: r.get(0)?,
                file_status: game_file_status(Path::new(&path)).into(),
                path,
                title: r.get(2)?,
                favorite: r.get::<_, i64>(3)? != 0,
                build_id: r.get(4)?,
                play_seconds: r.get(5)?,
                last_played: r.get(6)?,
                title_id: r.get(7)?,
                game_version: r.get(8)?,
                icon_path: r.get(9)?,
                install_size: r.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let result: Vec<Game> = rows
        .map(|row| row.map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    drop(s);
    let pending = result
        .iter()
        .filter(|entry| entry.install_size.is_none() && entry.file_status == "Available")
        .map(|entry| (entry.id, entry.path.clone()))
        .collect::<Vec<_>>();
    if !pending.is_empty() && !GAME_SIZE_REFRESH_RUNNING.swap(true, Ordering::AcqRel) {
        let app = app.clone();
        thread::spawn(move || {
            let scan_id = NEXT_GAME_SCAN_ID.fetch_add(1, Ordering::Relaxed);
            let total = pending.len() as u64;
            for (index, (game_id, path)) in pending.into_iter().enumerate() {
                if let Some(size) = calculate_game_size(Path::new(&path)) {
                    if let Ok(connection) = db(&app) {
                        let _ = connection.execute(
                            "UPDATE games SET install_size=? WHERE id=? AND install_size IS NULL",
                            params![size, game_id],
                        );
                    }
                }
                emit_game_scan(
                    &app,
                    GameScanEvent {
                        scan_id,
                        phase: "Calculating install sizes".into(),
                        completed: index as u64 + 1,
                        total,
                        current: path,
                        done: index as u64 + 1 == total,
                    },
                );
            }
            GAME_SIZE_REFRESH_RUNNING.store(false, Ordering::Release);
        });
    }
    Ok(result)
}
#[tauri::command]
fn game_compatibility(app: tauri::AppHandle, game_id: i64) -> Result<GameCompatibility, String> {
    let c = db(&app)?;
    let game = game(&c, game_id)?;
    let path_recognized = fs::canonicalize(&game.path)
        .map(|path| supported_game_path(&path))
        .unwrap_or(false);
    let (launch_attempts, emulator_process_starts): (i64, i64) = c
        .query_row(
            "SELECT COUNT(*),COALESCE(SUM(process_pid IS NOT NULL OR outcome IN ('normal','error','tracking_error')),0) FROM sessions WHERE game_id=?",
            [game_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let latest: Option<(Option<i32>, Option<String>, String)> = c
        .query_row(
            "SELECT exit_code,outcome,log_path FROM sessions WHERE game_id=? ORDER BY started_at DESC,id DESC LIMIT 1",
            [game_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (last_exit_code, last_outcome, emulator_reported_title_id, latest_log_path) =
        if let Some((exit_code, outcome, log_path)) = latest {
            let log_root = state_paths(&app)?.data.join("logs");
            let reported = log_reports_title_id(&log_root, Path::new(&log_path));
            (exit_code, outcome, reported, Some(log_path))
        } else {
            (None, None, false, None)
        };
    Ok(GameCompatibility {
        path_recognized,
        launch_attempts,
        emulator_process_starts,
        last_outcome,
        last_exit_code,
        emulator_reported_title_id,
        latest_log_path,
        gameplay_status: "Not measured by Studio".into(),
        playable_status: "Unconfirmed".into(),
    })
}
const OFFICIAL_COMPATIBILITY_INDEX_URL: &str = "https://kytyps5.github.io/data/compat-index.json";
const OFFICIAL_COMPATIBILITY_DETAILS_URL: &str = "https://kytyps5.github.io/data/compat/";
const OFFICIAL_COMPATIBILITY_CACHE_MAX_AGE: i64 = 6 * 60 * 60;
fn compatibility_cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = state_paths(app)?.cache.join("compatibility");
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}
fn compatibility_http_json(url: &str, max_bytes: usize) -> Result<serde_json::Value, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .header("User-Agent", "KytyPS5-Studio")
        .header("Accept", "application/json")
        .call()
        .map_err(|error| format!("Official compatibility database request failed: {error}"))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read official compatibility data: {error}"))?;
    if bytes.len() > max_bytes {
        return Err("Official compatibility response exceeded the size limit".into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Official compatibility data is invalid JSON: {error}"))
}

fn prioritize_compatibility_game(
    mut game: OfficialCompatibilityGame,
    host_os: &str,
) -> OfficialCompatibilityGame {
    let status = game.os.get(host_os).or_else(|| {
        game.os
            .iter()
            .filter(|(os, _)| os.trim().eq_ignore_ascii_case(host_os))
            .min_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, status)| status)
    });
    if let Some(status) = status {
        game.preferred_os = host_os.to_string();
        game.status = status.clone();
    } else {
        game.preferred_os = "all".into();
        game.status = game.overall.clone();
    }
    game
}

fn cached_compatibility_games(
    data: serde_json::Value,
    host_os: &str,
) -> Vec<OfficialCompatibilityGame> {
    serde_json::from_value::<Vec<OfficialCompatibilityGame>>(data)
        .unwrap_or_default()
        .into_iter()
        .map(|game| prioritize_compatibility_game(game, host_os))
        .collect()
}

fn current_host_compatibility_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn cached_json(path: &Path) -> Option<(i64, serde_json::Value)> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    Some((
        value.get("cached_at")?.as_i64()?,
        value.get("data")?.clone(),
    ))
}
fn write_cached_json(path: &Path, data: &serde_json::Value) -> Result<i64, String> {
    let cached_at = now();
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(&serde_json::json!({"cached_at":cached_at,"data":data}))
        .map_err(|error| error.to_string())?;
    fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    replace_file(&temp, path).map_err(|error| error.to_string())?;
    Ok(cached_at)
}
fn normalized_title_id(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_uppercase().replace('-', "");
    (value.len() == 9
        && value.starts_with("PPSA")
        && value[4..].bytes().all(|byte| byte.is_ascii_digit()))
    .then_some(value)
}
#[tauri::command]
fn official_compatibility_index(app: tauri::AppHandle) -> OfficialCompatibilityIndex {
    let Ok(cache_dir) = compatibility_cache_dir(&app) else {
        return OfficialCompatibilityIndex {
            available: false,
            updated_at: None,
            error: Some("Could not initialize the compatibility cache".into()),
            games: Vec::new(),
        };
    };
    let cache_path = cache_dir.join("index.json");
    let remote = compatibility_http_json(OFFICIAL_COMPATIBILITY_INDEX_URL, 8 * 1024 * 1024)
        .and_then(|value| {
            let payload: OfficialSiteIndexPayload =
                serde_json::from_value(value.clone()).map_err(|error| {
                    format!("Official compatibility index has an unexpected format: {error}")
                })?;
            if payload.games.is_empty() {
                return Err("Official compatibility index contained no game records".into());
            }
            let games = payload
                .games
                .into_iter()
                .filter_map(|game| {
                    let title_id =
                        normalized_title_id(game.title_id.as_deref().or(game.key.as_deref())?)?;
                    Some(prioritize_compatibility_game(
                        OfficialCompatibilityGame {
                            title_id,
                            title: game.title,
                            overall: game.overall.clone(),
                            status: game.overall,
                            preferred_os: "all".into(),
                            os: game.os,
                            report_counts: game.report_counts,
                            latest_tested: game.latest_tested,
                        },
                        current_host_compatibility_os(),
                    ))
                })
                .collect::<Vec<_>>();
            let data = serde_json::to_value(games).map_err(|error| error.to_string())?;
            let updated_at = write_cached_json(&cache_path, &data)?;
            Ok(OfficialCompatibilityIndex {
                available: true,
                updated_at: Some(updated_at),
                error: None,
                games: cached_compatibility_games(data, current_host_compatibility_os()),
            })
        });
    let error = match remote {
        Ok(index) => return index,
        Err(error) => error,
    };
    if let Some((updated_at, data)) = cached_json(&cache_path) {
        return OfficialCompatibilityIndex {
            available: true,
            updated_at: Some(updated_at),
            error: Some(error),
            games: cached_compatibility_games(data, current_host_compatibility_os()),
        };
    }
    OfficialCompatibilityIndex {
        available: false,
        updated_at: None,
        error: Some(error),
        games: Vec::new(),
    }
}
#[tauri::command]
fn official_compatibility_details(
    app: tauri::AppHandle,
    title_id: String,
) -> Result<Option<serde_json::Value>, String> {
    let Some(title_id) = normalized_title_id(&title_id) else {
        return Ok(None);
    };
    let cache_path = compatibility_cache_dir(&app)?.join(format!("{title_id}.json"));
    let cached = cached_json(&cache_path);
    if cached
        .as_ref()
        .is_some_and(|(updated_at, _)| now() - updated_at < OFFICIAL_COMPATIBILITY_CACHE_MAX_AGE)
    {
        return Ok(cached.map(|(_, data)| data));
    }
    let url = format!("{OFFICIAL_COMPATIBILITY_DETAILS_URL}{title_id}.json");
    match compatibility_http_json(&url, 2 * 1024 * 1024) {
        Ok(data) => {
            let returned_id = data
                .get("titleId")
                .or_else(|| data.get("title_id"))
                .and_then(|value| value.as_str())
                .and_then(normalized_title_id);
            if returned_id.as_deref() != Some(&title_id) {
                return cached.map(|(_, data)| Some(data)).ok_or_else(|| {
                    "Official compatibility report did not match the game's title ID".into()
                });
            }
            write_cached_json(&cache_path, &data)?;
            Ok(Some(data))
        }
        Err(error) => cached.map(|(_, data)| Some(data)).ok_or(error),
    }
}
fn compatibility_outcome(value: &str) -> bool {
    matches!(
        value,
        "launch_failed" | "reached_menu" | "reached_gameplay" | "playable" | "playable_with_issues"
    )
}
fn sanitize_compatibility_notes(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(2000)
        .collect()
}
fn patch_configuration_for_game(
    c: &Connection,
    game_id: i64,
    build: &Build,
) -> Result<String, String> {
    let mut options = build_profile_options(c, build.id)?;
    for (flag, value) in profile(c, game_id)?.options {
        options.insert(flag, value);
    }
    let capabilities = capabilities_for_build(build, &repo(c, build.repo_id)?)?;
    options.retain(|flag, _| {
        capabilities
            .verified_options
            .iter()
            .any(|supported| supported == flag)
    });
    let mut relevant: BTreeMap<_, _> = options
        .into_iter()
        .filter(|(flag, _)| {
            matches!(
                flag.as_str(),
                "--game-patch" | "--addcont-dir" | "--playgo-hack" | "--amd-cpu" | "--vr"
            )
        })
        .collect();
    if let Some(patch_path) = relevant.get("--game-patch") {
        let mut file = fs::File::open(patch_path)
            .map_err(|e| format!("Could not read game patch for compatibility record: {e}"))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 16384];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        relevant.insert(
            "--game-patch-sha256".into(),
            format!("{:x}", hasher.finalize()),
        );
    }
    serde_json::to_string(&relevant).map_err(|e| e.to_string())
}
#[tauri::command]
fn record_compatibility_observation(
    app: tauri::AppHandle,
    game_id: i64,
    outcome: String,
    notes: String,
) -> Result<(), String> {
    if !compatibility_outcome(&outcome) {
        return Err("Choose a supported manual test outcome".into());
    }
    let c = db(&app)?;
    game(&c, game_id)?;
    let b = build_for_game(&c, game_id)?;
    let patch_config = patch_configuration_for_game(&c, game_id, &b)?;
    c.execute("INSERT INTO manual_compatibility_observations(game_id,repo_id,build_id,commit_sha,build_type,patch_config_json,outcome,notes,observed_at) VALUES(?,?,?,?,?,?,?,?,?)", params![game_id,b.repo_id,b.id,b.commit_sha,b.build_type,patch_config,outcome,sanitize_compatibility_notes(&notes),now()]).map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn compatibility_observations(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<Vec<ManualCompatibilityObservation>, String> {
    let c = db(&app)?;
    game(&c, game_id)?;
    let mut statement = c.prepare("SELECT observation.id,observation.outcome,observation.notes,observation.observed_at,repositories.owner || '/' || repositories.name,observation.build_id,observation.commit_sha,observation.build_type,observation.patch_config_json FROM manual_compatibility_observations observation JOIN repositories ON repositories.id=observation.repo_id WHERE observation.game_id=? ORDER BY observation.observed_at DESC,observation.id DESC LIMIT 100").map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([game_id], |row| {
            Ok(ManualCompatibilityObservation {
                id: row.get(0)?,
                outcome: row.get(1)?,
                notes: row.get(2)?,
                observed_at: row.get(3)?,
                repository: row.get(4)?,
                build_id: row.get(5)?,
                build_commit: row.get(6)?,
                build_type: row.get(7)?,
                patch_config_json: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
fn log_reports_title_id(log_root: &Path, log_path: &Path) -> bool {
    let Ok(root) = fs::canonicalize(log_root) else {
        return false;
    };
    let Ok(path) = fs::canonicalize(log_path) else {
        return false;
    };
    if !path_starts_with(&path, &root) {
        return false;
    }
    fs::metadata(&path)
        .map(|metadata| metadata.is_file() && metadata.len() <= 8 * 1024 * 1024)
        .unwrap_or(false)
        && fs::read_to_string(path)
            .map(|contents| contents.lines().any(|line| line.contains("Title ID:")))
            .unwrap_or(false)
}
#[tauri::command]
fn add_repository(
    app: tauri::AppHandle,
    url: String,
    branch: Option<String>,
) -> Result<Repo, String> {
    let (owner, name, url) = parse_repo(&url)?;
    let branch = branch.unwrap_or_default();
    if !branch.is_empty() && (branch.len() > 255 || !valid_branch_name(&branch)) {
        return Err("Invalid repository default branch".into());
    }
    let c = db(&app)?;
    c.execute(
        "INSERT INTO repositories(owner,name,url,branch) VALUES(?,?,?,?) ON CONFLICT(url) DO UPDATE SET removed_at=NULL,branch=CASE WHEN excluded.branch<>'' THEN excluded.branch ELSE repositories.branch END",
        params![owner, name, url, branch],
    )
    .map_err(|e| e.to_string())?;
    let id: i64 = c
        .query_row("SELECT id FROM repositories WHERE url=?", [url], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    repo(&c, id)
}
#[tauri::command]
fn remove_repository(app: tauri::AppHandle, id: i64) -> Result<String, String> {
    if id <= 0 {
        return Err("Repository ID must be positive".into());
    }
    let c = db(&app)?;
    let repository = repo(&c, id)?;
    c.execute(
        "UPDATE repositories SET removed_at=? WHERE id=? AND removed_at IS NULL",
        params![now(), id],
    )
    .map_err(|error| error.to_string())?;
    Ok(if repository.source_path.is_some() {
        format!("Removed {}/{} from tracked repositories. Existing source files, builds, and history were kept.", repository.owner, repository.name)
    } else {
        format!(
            "Removed {}/{} from tracked repositories.",
            repository.owner, repository.name
        )
    })
}
#[derive(Deserialize, Serialize)]
struct GithubRepo {
    full_name: String,
    description: Option<String>,
    stargazers_count: u64,
    updated_at: String,
    default_branch: String,
    html_url: String,
    fork: bool,
    license: Option<GithubLicense>,
}
#[derive(Deserialize, Serialize)]
struct GithubLicense {
    spdx_id: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct GithubRemoteRepo {
    default_branch: String,
    fork: bool,
    stargazers_count: u64,
    updated_at: String,
}
#[derive(Deserialize, Serialize)]
struct GithubCommit {
    sha: String,
    commit: GithubCommitBody,
}
#[derive(Deserialize, Serialize)]
struct GithubCommitBody {
    message: String,
    author: Option<GithubCommitAuthor>,
}
#[derive(Deserialize, Serialize)]
struct GithubCommitAuthor {
    date: String,
}
#[derive(Deserialize, Serialize)]
struct GithubRelease {
    name: Option<String>,
    tag_name: String,
    published_at: Option<String>,
    html_url: String,
    prerelease: bool,
    draft: bool,
}
#[derive(Deserialize)]
struct GithubAppRelease {
    tag_name: String,
    html_url: String,
    prerelease: bool,
    draft: bool,
    assets: Vec<GithubAppReleaseAsset>,
}
#[derive(Deserialize)]
struct GithubAppReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}
#[derive(Serialize)]
struct AppUpdateInfo {
    current_version: String,
    latest_version: String,
    update_available: bool,
    release_url: String,
    asset_name: Option<String>,
    download_size_bytes: Option<u64>,
    running_appimage: bool,
}
#[derive(Serialize)]
struct GithubRepositoryDetails {
    default_branch: String,
    fork: bool,
    stars: u64,
    updated_at: String,
    latest_commit: Option<GithubCommit>,
    releases: Vec<GithubRelease>,
}
#[derive(Deserialize)]
struct GithubPrUser {
    login: String,
}
#[derive(Deserialize)]
struct GithubPrRepo {
    full_name: String,
}
#[derive(Deserialize)]
struct GithubPrBranch {
    #[serde(rename = "ref")]
    branch: String,
    sha: String,
    repo: Option<GithubPrRepo>,
}
#[derive(Deserialize)]
struct GithubPrLabel {
    name: String,
}
#[derive(Deserialize)]
struct GithubPullRequestApi {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    #[serde(default)]
    draft: bool,
    updated_at: String,
    #[serde(default)]
    created_at: String,
    html_url: String,
    user: Option<GithubPrUser>,
    base: GithubPrBranch,
    head: GithubPrBranch,
    #[serde(default)]
    labels: Vec<GithubPrLabel>,
    #[serde(default)]
    mergeable: Option<bool>,
    #[serde(default)]
    mergeable_state: Option<String>,
    #[serde(default)]
    changed_files: Option<u32>,
    #[serde(default)]
    additions: Option<u32>,
    #[serde(default)]
    deletions: Option<u32>,
    merged_at: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
struct CommunityPullRequest {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    merged: bool,
    draft: bool,
    updated_at: String,
    #[serde(default)]
    created_at: String,
    html_url: String,
    author: String,
    base_repo: Option<String>,
    base_branch: String,
    base_sha: String,
    head_repo: Option<String>,
    head_branch: String,
    head_sha: String,
    labels: Vec<String>,
    mergeable: Option<bool>,
    mergeable_state: Option<String>,
    changed_files: Option<u32>,
    additions: Option<u32>,
    deletions: Option<u32>,
    build_compatibility: String,
}
impl From<GithubPullRequestApi> for CommunityPullRequest {
    fn from(pr: GithubPullRequestApi) -> Self {
        Self {
            number: pr.number,
            title: pr.title,
            body: pr.body,
            state: pr.state,
            merged: pr.merged_at.is_some(),
            draft: pr.draft,
            updated_at: pr.updated_at,
            created_at: pr.created_at,
            html_url: pr.html_url,
            author: pr.user.map(|user| user.login).unwrap_or_default(),
            base_repo: pr.base.repo.map(|repo| repo.full_name),
            base_branch: pr.base.branch,
            base_sha: pr.base.sha,
            head_repo: pr.head.repo.map(|repo| repo.full_name),
            head_branch: pr.head.branch,
            head_sha: pr.head.sha,
            labels: pr.labels.into_iter().map(|label| label.name).collect(),
            mergeable: pr.mergeable,
            mergeable_state: pr.mergeable_state,
            changed_files: pr.changed_files,
            additions: pr.additions,
            deletions: pr.deletions,
            build_compatibility: "Unverified".into(),
        }
    }
}
#[derive(Serialize)]
struct CommunityPrPage {
    repository: String,
    items: Vec<CommunityPullRequest>,
    total_count: Option<u64>,
    page: u32,
    per_page: u32,
    has_more: bool,
    truncated: bool,
    stale: bool,
    refresh_error: Option<String>,
}
#[derive(Deserialize)]
struct CommunityBuildSelection {
    number: u64,
    head_sha: String,
    base_sha: String,
}
#[derive(Deserialize)]
struct GithubSearchCount {
    total_count: u64,
}
#[derive(Deserialize)]
struct GithubSearchPullRequests {
    total_count: u64,
    items: Vec<GithubSearchPullRequest>,
}
#[derive(Deserialize)]
struct GithubSearchPullRequest {
    number: u64,
}
#[derive(Deserialize)]
struct GithubCheckRuns {
    #[serde(default)]
    check_runs: Vec<GithubCheckRun>,
}
#[derive(Deserialize, Serialize)]
struct GithubCheckRun {
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: String,
}
#[derive(Deserialize)]
struct GithubPrFile {
    filename: String,
    status: String,
    additions: u32,
    deletions: u32,
    patch: Option<String>,
}
#[derive(Serialize)]
struct CommunityPrFile {
    filename: String,
    status: String,
    additions: u32,
    deletions: u32,
    patch: Option<String>,
}
#[derive(Serialize)]
struct CommunityPrDetails {
    pull_request: CommunityPullRequest,
    commits: Vec<GithubCommit>,
    files: Vec<CommunityPrFile>,
    checks: Vec<GithubCheckRun>,
    checks_state: String,
    subresource_errors: Vec<String>,
}

fn validate_github_token(token: &str) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty() || token.len() > 4096 || token.chars().any(char::is_control) {
        return Err("GitHub token must be 1 to 4096 characters with no control characters".into());
    }
    Ok(token.to_string())
}
fn github_token() -> Result<Option<String>, String> {
    for variable in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(token) = env::var(variable) {
            if let Ok(token) = validate_github_token(&token) {
                return Ok(Some(token));
            }
        }
    }
    #[cfg(unix)]
    {
        let token = {
            let output = match Command::new("secret-tool")
                .args(["lookup", "service", "kytyps5-studio", "account", "github"])
                .output()
            {
                Ok(output) => output,
                Err(_) => return Ok(None),
            };
            if !output.status.success() {
                return Ok(None);
            }
            String::from_utf8(output.stdout).map_err(|error| error.to_string())?
        };
        Ok(Some(validate_github_token(
            token.trim_end_matches(['\r', '\n']),
        )?))
    }
    #[cfg(windows)]
    {
        let Some(token) = windows_credentials::get()? else {
            return Ok(None);
        };
        Ok(Some(validate_github_token(
            token.trim_end_matches(['\r', '\n']),
        )?))
    }
}
#[tauri::command]
fn save_github_token(token: String) -> Result<(), String> {
    let token = validate_github_token(&token)?;
    #[cfg(unix)]
    {
        let mut child = Command::new("secret-tool")
            .args([
                "store",
                "--label=KytyPS5 Studio GitHub token",
                "service",
                "kytyps5-studio",
                "account",
                "github",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Linux Secret Service is unavailable: {error}"))?;
        child
            .stdin
            .take()
            .ok_or("Could not open secure credential input")?
            .write_all(format!("{token}\n").as_bytes())
            .map_err(|error| error.to_string())?;
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Could not store the token in Secret Service: {detail}"
            ));
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        windows_credentials::save(&token)
    }
}
#[tauri::command]
fn delete_github_token() -> Result<(), String> {
    #[cfg(unix)]
    {
        let status = Command::new("secret-tool")
            .args(["clear", "service", "kytyps5-studio", "account", "github"])
            .status()
            .map_err(|error| format!("Linux Secret Service is unavailable: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err("Could not remove the GitHub token from Secret Service".into())
        }
    }
    #[cfg(windows)]
    {
        windows_credentials::delete()
    }
}
#[tauri::command]
fn github_token_status() -> Result<bool, String> {
    Ok(github_token()?.is_some())
}
fn github_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .new_agent();
    let mut request = agent
        .get(url)
        .header("User-Agent", "KytyPS5-Studio")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = github_token()? {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let mut response = request.call().map_err(|error| {
        let message = error.to_string();
        if message.contains("401") {
            "GitHub rejected the configured token".to_string()
        } else if message.contains("403") || message.to_ascii_lowercase().contains("rate limit") {
            "GitHub API rate limit reached; try again later".to_string()
        } else {
            format!("GitHub request failed: {message}")
        }
    })?;
    response
        .body_mut()
        .read_json()
        .map_err(|error| format!("GitHub returned invalid JSON: {error}"))
}
const APP_RELEASES_API: &str = "https://api.github.com/repos/Febsho/KytyPs5-Studio/releases/latest";
const APP_RELEASES_DOWNLOAD_PREFIX: &str =
    "https://github.com/Febsho/KytyPs5-Studio/releases/download/";
const MAX_APP_UPDATE_ASSET_SIZE: u64 = 2 * 1024 * 1024 * 1024;

fn app_update_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_body(Some(Duration::from_secs(120)))
        .build()
        .new_agent()
}

fn release_version_tuple(value: &str) -> Option<Vec<u64>> {
    value
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let (Some(latest), Some(current)) = (
        release_version_tuple(latest),
        release_version_tuple(current),
    ) else {
        return false;
    };
    (0..latest.len().max(current.len()))
        .map(|index| {
            (
                latest.get(index).copied().unwrap_or_default(),
                current.get(index).copied().unwrap_or_default(),
            )
        })
        .find_map(|(latest, current)| (latest != current).then_some(latest > current))
        .unwrap_or(false)
}

#[cfg(unix)]
fn running_appimage_path() -> Option<PathBuf> {
    let path = env::var_os("APPIMAGE").map(PathBuf::from)?;
    if !path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("appimage"))
    {
        return None;
    }
    let metadata = fs::symlink_metadata(&path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    let canonical = fs::canonicalize(path).ok()?;
    canonical.is_file().then_some(canonical)
}

fn latest_app_release() -> Result<GithubAppRelease, String> {
    let release: GithubAppRelease = github_json(APP_RELEASES_API)?;
    if release.draft || release.prerelease {
        return Err("GitHub's latest Studio release is not a stable public release".into());
    }
    Ok(release)
}

#[cfg(unix)]
fn appimage_asset(release: &GithubAppRelease) -> Option<&GithubAppReleaseAsset> {
    let name = match env::consts::ARCH {
        "x86_64" => ["x86_64", "amd64"],
        "aarch64" => ["aarch64", "arm64"],
        _ => return None,
    };
    release.assets.iter().find(|asset| {
        let lower = asset.name.to_ascii_lowercase();
        lower.ends_with(".appimage") && name.iter().any(|arch| lower.contains(arch))
    })
}

#[cfg(any(windows, test))]
fn nsis_asset(release: &GithubAppRelease) -> Option<&GithubAppReleaseAsset> {
    release.assets.iter().find(|asset| {
        let lower = asset.name.to_ascii_lowercase();
        lower.ends_with("-setup.exe")
            && (lower.contains("x64") || lower.contains("x86_64") || lower.contains("amd64"))
    })
}

#[cfg(any(windows, test))]
fn nsis_update_arguments(current_executable: &Path) -> Result<[OsString; 4], String> {
    let install_directory = current_executable
        .parent()
        .filter(|directory| directory.is_absolute())
        .ok_or("Could not determine the absolute application install directory")?;
    let mut install_directory_argument = OsString::from("/D=");
    install_directory_argument.push(install_directory.as_os_str());
    Ok([
        OsString::from("/UPDATE"),
        OsString::from("/P"),
        OsString::from("/R"),
        install_directory_argument,
    ])
}

#[tauri::command]
fn check_app_update() -> Result<AppUpdateInfo, String> {
    let release = latest_app_release()?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    #[cfg(unix)]
    let asset = appimage_asset(&release);
    #[cfg(windows)]
    let asset = nsis_asset(&release);
    let latest_version = release.tag_name.clone();
    let release_url = release.html_url.clone();
    let update_available = version_is_newer(&latest_version, &current_version);
    let asset_name = asset.map(|asset| asset.name.clone());
    let download_size_bytes = asset.map(|asset| asset.size);
    Ok(AppUpdateInfo {
        update_available,
        current_version,
        latest_version,
        release_url,
        asset_name,
        download_size_bytes,
        running_appimage: {
            #[cfg(unix)]
            {
                running_appimage_path().is_some()
            }
            #[cfg(windows)]
            {
                false
            }
        },
    })
}

fn release_asset_url(
    release: &GithubAppRelease,
    asset: &GithubAppReleaseAsset,
) -> Result<String, String> {
    let url = url::Url::parse(&asset.browser_download_url).map_err(|error| error.to_string())?;
    let expected_url = url::Url::parse(&format!(
        "{APP_RELEASES_DOWNLOAD_PREFIX}{}/{}",
        release.tag_name, asset.name
    ))
    .map_err(|error| error.to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.path() != expected_url.path()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !asset
            .browser_download_url
            .starts_with(APP_RELEASES_DOWNLOAD_PREFIX)
    {
        return Err("GitHub returned an unexpected release download URL".into());
    }
    Ok(asset.browser_download_url.clone())
}

fn release_asset_text(
    asset: &GithubAppReleaseAsset,
    release: &GithubAppRelease,
) -> Result<String, String> {
    let url = release_asset_url(release, asset)?;
    let request = app_update_agent()
        .get(&url)
        .header("User-Agent", "KytyPS5-Studio")
        .header("Accept", "application/octet-stream");
    let mut response = request
        .call()
        .map_err(|error| format!("Could not download release checksums: {error}"))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
fn download_and_install_app_update() -> Result<String, String> {
    let target = running_appimage_path().ok_or(
        "In-place updates are available only when KytyPS5 Studio is running from an AppImage",
    )?;
    let release = latest_app_release()?;
    let current_version = env!("CARGO_PKG_VERSION");
    if !version_is_newer(&release.tag_name, current_version) {
        return Err("KytyPS5 Studio is already up to date".into());
    }
    let asset = appimage_asset(&release)
        .ok_or("This release has no AppImage for the current system architecture")?;
    if asset.size == 0 || asset.size > MAX_APP_UPDATE_ASSET_SIZE {
        return Err("The AppImage release asset has an invalid or excessive size".into());
    }
    let checksums = release
        .assets
        .iter()
        .find(|candidate| candidate.name == "SHA256SUMS")
        .ok_or("The release does not include SHA256SUMS; update was not installed")?;
    let expected_digest = if let Some(digest) = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
    {
        digest.to_ascii_lowercase()
    } else {
        release_asset_text(checksums, &release)?
            .lines()
            .find_map(|line| {
                let mut fields = line.split_whitespace();
                let digest = fields.next()?;
                let name = fields.next()?.trim_start_matches('*');
                (name == asset.name).then(|| digest.to_ascii_lowercase())
            })
            .ok_or("SHA256SUMS does not contain the AppImage checksum")?
    };
    if expected_digest.len() != 64
        || !expected_digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("The release AppImage checksum is malformed".into());
    }
    let parent = target.parent().ok_or("AppImage has no parent directory")?;
    let filename = target
        .file_name()
        .ok_or("AppImage has no filename")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{filename}.{}.download", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let result = (|| {
        let url = release_asset_url(&release, asset)?;
        let request = app_update_agent()
            .get(&url)
            .header("User-Agent", "KytyPS5-Studio")
            .header("Accept", "application/octet-stream");
        let mut response = request
            .call()
            .map_err(|error| format!("Could not download AppImage: {error}"))?;
        let mut reader = response.body_mut().as_reader();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create temporary update file: {error}"))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        let mut total = 0_u64;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > MAX_APP_UPDATE_ASSET_SIZE || total > asset.size {
                return Err("Downloaded AppImage exceeded its declared size".into());
            }
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            hasher.update(&buffer[..read]);
        }
        if total != asset.size {
            return Err(format!(
                "Downloaded AppImage size mismatch: expected {}, received {total}",
                asset.size
            ));
        }
        let mut signature = [0_u8; 4];
        let mut verify_file = fs::File::open(&temporary).map_err(|error| error.to_string())?;
        verify_file
            .read_exact(&mut signature)
            .map_err(|error| format!("Downloaded asset is not a readable AppImage: {error}"))?;
        if signature != *b"\x7fELF" {
            return Err("Downloaded asset is not an ELF AppImage; current app was kept".into());
        }
        let digest = format!("{:x}", hasher.finalize());
        if digest != expected_digest {
            return Err(
                "Downloaded AppImage checksum did not match GitHub's release checksum".into(),
            );
        }
        file.sync_all().map_err(|error| error.to_string())?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        fs::rename(&temporary, &target)
            .map_err(|error| format!("Verified update could not replace the AppImage: {error}"))?;
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(format!(
            "Updated KytyPS5 Studio to {}. Restarting Studio now.",
            release.tag_name
        ))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(any(windows, test))]
struct UpdateReservation {
    jobs: Arc<Jobs>,
    preserve: bool,
}

#[cfg(any(windows, test))]
impl Drop for UpdateReservation {
    fn drop(&mut self) {
        if !self.preserve {
            self.jobs.updating.store(false, Ordering::SeqCst);
        }
    }
}

#[cfg(windows)]
fn download_and_install_app_update(app: tauri::AppHandle) -> Result<String, String> {
    let _session_lifecycle = SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    let jobs = app.state::<Arc<Jobs>>();
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if jobs.active.load(Ordering::SeqCst) {
        return Err("Finish the active build before installing a Studio update".into());
    }
    if jobs.updating.swap(true, Ordering::SeqCst) {
        return Err("A KytyPS5 Studio update is already in progress".into());
    }
    let mut reservation = UpdateReservation {
        jobs: jobs.inner().clone(),
        preserve: false,
    };
    drop(_control);
    let connection = db(&app)?;
    if has_active_game_sessions(&connection)? {
        return Err("Close the active emulator session before installing a Studio update".into());
    }
    let current_executable = env::current_exe()
        .map_err(|error| format!("Could not locate the running application: {error}"))?;
    let update_arguments = nsis_update_arguments(&current_executable)?;
    let release = latest_app_release()?;
    let current_version = env!("CARGO_PKG_VERSION");
    if !version_is_newer(&release.tag_name, current_version) {
        return Err("KytyPS5 Studio is already up to date".into());
    }
    let asset = nsis_asset(&release).ok_or("This release has no NSIS installer for Windows x64")?;
    if asset.size == 0 || asset.size > MAX_APP_UPDATE_ASSET_SIZE {
        return Err("The NSIS release asset has an invalid or excessive size".into());
    }
    let expected_digest = if let Some(digest) = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
    {
        digest.to_ascii_lowercase()
    } else {
        let checksums = release
            .assets
            .iter()
            .find(|candidate| candidate.name == "SHA256SUMS")
            .ok_or("The release does not include SHA256SUMS; update was not installed")?;
        release_asset_text(checksums, &release)?
            .lines()
            .find_map(|line| {
                let mut fields = line.split_whitespace();
                let digest = fields.next()?;
                let name = fields.next()?.trim_start_matches('*');
                (name == asset.name).then(|| digest.to_ascii_lowercase())
            })
            .ok_or("SHA256SUMS does not contain the NSIS installer checksum")?
    };
    if expected_digest.len() != 64 || !expected_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("GitHub returned an invalid NSIS installer SHA-256 checksum".into());
    }
    let url = release_asset_url(&release, asset)?;
    let mut response = ureq::get(&url)
        .header("User-Agent", "KytyPS5-Studio")
        .header("Accept", "application/octet-stream")
        .call()
        .map_err(|error| format!("Could not download the NSIS installer: {error}"))?;
    let update_dir = state_paths(&app)?.cache.join("updates");
    fs::create_dir_all(&update_dir)
        .map_err(|error| format!("Could not create the update staging directory: {error}"))?;
    let canonical_cache = fs::canonicalize(state_paths(&app)?.cache)
        .map_err(|error| format!("Managed cache directory is unavailable: {error}"))?;
    let canonical_update = fs::canonicalize(&update_dir)
        .map_err(|error| format!("Update staging directory is unavailable: {error}"))?;
    if !path_starts_with(&canonical_update, &canonical_cache)
        || fs::symlink_metadata(&update_dir)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("Update staging directory is outside the managed cache".into());
    }
    let tag: String = release
        .tag_name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        .take(64)
        .collect();
    let installer = update_dir.join(format!("KytyPS5-Studio-{tag}-{}.exe", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&installer)
        .map_err(|error| format!("Could not create the staged NSIS installer: {error}"))?;
    let result = (|| -> Result<(), String> {
        let mut hasher = Sha256::new();
        let mut reader = response.body_mut().as_reader();
        let mut buffer = [0_u8; 128 * 1024];
        let mut total = 0_u64;
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            total = total.saturating_add(count as u64);
            if total > MAX_APP_UPDATE_ASSET_SIZE || total > asset.size {
                return Err("Downloaded NSIS installer exceeded its declared size".into());
            }
            file.write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            hasher.update(&buffer[..count]);
        }
        if total != asset.size {
            return Err(format!(
                "NSIS installer size mismatch: expected {}, received {total}",
                asset.size
            ));
        }
        if format!("{:x}", hasher.finalize()) != expected_digest {
            return Err("NSIS installer checksum did not match the GitHub release checksum".into());
        }
        let mut signature = [0_u8; 2];
        let mut verification = fs::File::open(&installer).map_err(|error| error.to_string())?;
        verification
            .read_exact(&mut signature)
            .map_err(|error| format!("Could not validate downloaded NSIS installer: {error}"))?;
        if signature != *b"MZ" {
            return Err("Downloaded NSIS installer is not a Windows executable".into());
        }
        file.sync_all().map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&installer);
        return Err(error);
    }
    if let Err(error) = Command::new(&installer).args(&update_arguments).spawn() {
        let _ = fs::remove_file(&installer);
        return Err(format!(
            "Could not launch the verified NSIS installer: {error}"
        ));
    }
    reservation.preserve = true;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        app.exit(0);
    });
    Ok(format!(
        "Verified KytyPS5 Studio {} installer started. Studio will close, install the update, and restart automatically.",
        release.tag_name
    ))
}

#[cfg(any(windows, test))]
fn has_active_game_sessions(connection: &Connection) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE ended_at IS NULL)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not verify active emulator sessions: {error}"))
}

#[tauri::command]
#[cfg(unix)]
async fn install_app_update(app: tauri::AppHandle) -> Result<String, String> {
    let result = tauri::async_runtime::spawn_blocking(download_and_install_app_update)
        .await
        .map_err(|error| format!("AppImage update worker failed: {error}"))??;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        app.restart();
    });
    Ok(result)
}
#[tauri::command]
#[cfg(windows)]
fn install_app_update(app: tauri::AppHandle) -> Result<String, String> {
    download_and_install_app_update(app)
}
fn cached_community_prs(
    conn: &Connection,
    repository_id: i64,
) -> Result<Vec<CommunityPullRequest>, String> {
    let mut statement = conn
        .prepare("SELECT payload FROM community_pr_cache WHERE repo_id=? ORDER BY cached_at DESC")
        .map_err(|error| error.to_string())?;
    let payloads = statement
        .query_map([repository_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut prs = Vec::new();
    for payload in payloads {
        if let Ok(pr) = serde_json::from_str::<CommunityPullRequest>(
            &payload.map_err(|error| error.to_string())?,
        ) {
            prs.push(pr);
        }
    }
    prs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(prs)
}
fn community_pr_total(owner: &str, name: &str, state: &str) -> Result<u64, String> {
    let state_query = match state {
        "open" => " state:open",
        "closed" => " state:closed",
        _ => "",
    };
    let query = format!("repo:{owner}/{name} is:pr{state_query}");
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", &query)
        .append_pair("per_page", "1")
        .finish();
    let response: GithubSearchCount =
        github_json(&format!("https://api.github.com/search/issues?{encoded}"))?;
    Ok(response.total_count)
}
fn github_pull_request_search_url(
    owner: &str,
    name: &str,
    state: &str,
    search: &str,
    page: u32,
    per_page: usize,
) -> String {
    let state_qualifier = match state {
        "open" => " state:open",
        "closed" => " state:closed",
        _ => "",
    };
    let query = format!("repo:{owner}/{name} is:pr{state_qualifier} {search}");
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", &query)
        .append_pair("per_page", &per_page.to_string())
        .append_pair("page", &page.to_string())
        .finish();
    format!("https://api.github.com/search/issues?{encoded}")
}
fn github_search_has_more(page: u32, page_size: u64, total_count: u64) -> bool {
    u64::from(page) * page_size < total_count.min(1000)
}
#[tauri::command]
fn list_community_pull_requests(
    app: tauri::AppHandle,
    repository_id: i64,
    page: u32,
    state: String,
    search: String,
) -> Result<CommunityPrPage, String> {
    if page == 0 || page > 1000 {
        return Err("Pull request page must be between 1 and 1000".into());
    }
    if !["open", "closed", "all"].contains(&state.as_str()) {
        return Err("Pull request state must be open, closed, or all".into());
    }
    if search.len() > 128 || search.chars().any(char::is_control) {
        return Err(
            "Pull request search must be at most 128 characters without control characters".into(),
        );
    }
    let conn = db(&app)?;
    let repository = repo(&conn, repository_id)?;
    if repository.source_path.is_none() {
        return Err("Clone the selected repository before browsing its pull requests".into());
    }
    let (owner, name, _) = parse_repo(&repository.url)?;
    if !search.trim().is_empty() {
        return search_community_pull_requests(
            &conn,
            repository_id,
            &owner,
            &name,
            &state,
            search.trim(),
            page,
        );
    }
    let endpoint = format!(
        "https://api.github.com/repos/{owner}/{name}/pulls?state={state}&sort=updated&direction=desc&per_page=100&page={page}"
    );
    let live = github_json::<Vec<GithubPullRequestApi>>(&endpoint);
    if let Ok(api_prs) = live {
        let items: Vec<CommunityPullRequest> = api_prs.into_iter().map(Into::into).collect();
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        for pr in &items {
            let payload = serde_json::to_string(pr).map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO community_pr_cache(repo_id,number,payload,cached_at) VALUES(?,?,?,?) ON CONFLICT(repo_id,number) DO UPDATE SET payload=excluded.payload,cached_at=excluded.cached_at",
                params![repository_id, pr.number as i64, payload, now()],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        let (total_count, refresh_error) = match community_pr_total(&owner, &name, &state) {
            Ok(count) => (Some(count), None),
            Err(error) => (None, Some(error)),
        };
        let has_more = total_count
            .map(|count| u64::from(page) * 100 < count)
            .unwrap_or(items.len() == 100);
        return Ok(CommunityPrPage {
            repository: format!("{owner}/{name}"),
            items,
            total_count,
            page,
            per_page: 100,
            has_more,
            truncated: false,
            stale: false,
            refresh_error,
        });
    }
    let error = live.err().expect("successful response returned earlier");
    let mut cached = cached_community_prs(&conn, repository_id)?;
    cached.retain(|pr| match state.as_str() {
        "open" => pr.state == "open",
        "closed" => pr.state == "closed",
        _ => true,
    });
    if cached.is_empty() {
        return Err(error);
    }
    let total_cached = cached.len();
    let start = ((page - 1) as usize).saturating_mul(100);
    let items = cached
        .drain(start.min(total_cached)..)
        .take(40)
        .collect::<Vec<_>>();
    let has_more = start.saturating_add(items.len()) < total_cached;
    Ok(CommunityPrPage {
        repository: format!("{owner}/{name}"),
        items,
        total_count: None,
        page,
        per_page: 100,
        has_more,
        truncated: false,
        stale: true,
        refresh_error: Some(error),
    })
}
fn search_community_pull_requests(
    conn: &Connection,
    repository_id: i64,
    owner: &str,
    name: &str,
    state: &str,
    search: &str,
    page: u32,
) -> Result<CommunityPrPage, String> {
    const SEARCH_PAGE_SIZE: usize = 20;
    if page > 50 {
        return Err(
            "GitHub search can page through only its first 1000 matches; refine the search".into(),
        );
    }
    let url = github_pull_request_search_url(owner, name, state, search, page, SEARCH_PAGE_SIZE);
    let results: GithubSearchPullRequests = match github_json(&url) {
        Ok(results) => results,
        Err(error) => {
            let mut cached = cached_community_prs(conn, repository_id)?;
            let needle = search.to_lowercase();
            cached.retain(|pr| {
                (state == "all" || pr.state == state)
                    && (pr.title.to_lowercase().contains(&needle)
                        || pr.author.to_lowercase().contains(&needle)
                        || pr
                            .body
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&needle))
            });
            let total_count = cached.len();
            let truncated = total_count > 1000;
            let start = ((page - 1) as usize).saturating_mul(SEARCH_PAGE_SIZE);
            let items = cached
                .drain(start.min(total_count)..)
                .take(SEARCH_PAGE_SIZE)
                .collect::<Vec<_>>();
            let has_more = start.saturating_add(items.len()) < total_count.min(1000);
            return Ok(CommunityPrPage {
                repository: format!("{owner}/{name}"),
                items,
                total_count: None,
                page,
                per_page: (SEARCH_PAGE_SIZE as u32),
                has_more,
                truncated,
                stale: true,
                refresh_error: Some(error),
            });
        }
    };
    let mut items = Vec::with_capacity(results.items.len());
    for item in results.items {
        let pr: GithubPullRequestApi = github_json(&format!(
            "https://api.github.com/repos/{owner}/{name}/pulls/{}",
            item.number
        ))?;
        items.push(pr.into());
    }
    cache_community_pull_requests(conn, repository_id, &items)?;
    let total_count = results.total_count;
    Ok(CommunityPrPage {
        repository: format!("{owner}/{name}"),
        has_more: github_search_has_more(page, SEARCH_PAGE_SIZE as u64, total_count),
        items,
        total_count: Some(total_count),
        page,
        per_page: (SEARCH_PAGE_SIZE as u32),
        truncated: total_count > 1000,
        stale: false,
        refresh_error: None,
    })
}
fn cache_community_pull_requests(
    conn: &Connection,
    repository_id: i64,
    items: &[CommunityPullRequest],
) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    for pr in items {
        let payload = serde_json::to_string(pr).map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO community_pr_cache(repo_id,number,payload,cached_at) VALUES(?,?,?,?) ON CONFLICT(repo_id,number) DO UPDATE SET payload=excluded.payload,cached_at=excluded.cached_at",
            params![repository_id, pr.number as i64, payload, now()],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}
#[tauri::command]
fn community_pull_request_details(
    app: tauri::AppHandle,
    repository_id: i64,
    number: u64,
) -> Result<CommunityPrDetails, String> {
    if number == 0 || number > i64::MAX as u64 {
        return Err("Pull request number is invalid".into());
    }
    let conn = db(&app)?;
    let repository = repo(&conn, repository_id)?;
    let (owner, name, _) = parse_repo(&repository.url)?;
    let root = format!("https://api.github.com/repos/{owner}/{name}/pulls/{number}");
    let api_pr: GithubPullRequestApi = github_json(&root)?;
    let pull_request = CommunityPullRequest::from(api_pr);
    let mut subresource_errors = Vec::new();
    let commits = match github_json::<Vec<GithubCommit>>(&format!("{root}/commits?per_page=100")) {
        Ok(commits) => commits,
        Err(error) => {
            subresource_errors.push(format!("Commit history unavailable: {error}"));
            Vec::new()
        }
    };
    let files = match github_json::<Vec<GithubPrFile>>(&format!("{root}/files?per_page=100")) {
        Ok(files) => files
            .into_iter()
            .map(|file| CommunityPrFile {
                filename: file.filename,
                status: file.status,
                additions: file.additions,
                deletions: file.deletions,
                patch: file.patch,
            })
            .collect(),
        Err(error) => {
            subresource_errors.push(format!("Changed files unavailable: {error}"));
            Vec::new()
        }
    };
    let checks_response = github_json::<GithubCheckRuns>(&format!(
        "https://api.github.com/repos/{owner}/{name}/commits/{}/check-runs?per_page=100",
        pull_request.head_sha
    ));
    let (checks, checks_state) = match checks_response {
        Ok(response) => {
            let state = if response.check_runs.is_empty() {
                "No checks reported".to_string()
            } else if response.check_runs.iter().any(|check| {
                check.status == "completed"
                    && matches!(
                        check.conclusion.as_deref(),
                        Some("failure" | "timed_out" | "cancelled" | "action_required")
                    )
            }) {
                "Failure".to_string()
            } else if response
                .check_runs
                .iter()
                .any(|check| check.status != "completed")
            {
                "Pending".to_string()
            } else {
                "Success".to_string()
            };
            (response.check_runs, state)
        }
        Err(error) => {
            subresource_errors.push(format!("CI checks unavailable: {error}"));
            (Vec::new(), "Unavailable".into())
        }
    };
    Ok(CommunityPrDetails {
        pull_request,
        commits,
        files,
        checks,
        checks_state,
        subresource_errors,
    })
}
#[tauri::command]
fn start_community_build(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    repository_id: i64,
    pull_requests: Vec<CommunityBuildSelection>,
    options: BuildOptions,
) -> Result<u64, String> {
    let compiler = validate_build_options(&options)?;
    if pull_requests.is_empty()
        || pull_requests.len() > 12
        || pull_requests.iter().any(|selection| {
            selection.number == 0
                || selection.number > i64::MAX as u64
                || selection.head_sha.len() != 40
                || !selection
                    .head_sha
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || selection.base_sha.len() != 40
                || !selection
                    .base_sha
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err("Select between 1 and 12 valid pull requests".into());
    }
    let mut unique = HashSet::new();
    if pull_requests
        .iter()
        .any(|selection| !unique.insert(selection.number))
    {
        return Err("A pull request can only appear once in the merge order".into());
    }
    let connection = db(&app)?;
    let repository = repo(&connection, repository_id)?;
    if !["official", "kyty-cmake"].contains(&repository.adapter.as_str()) {
        return Err("No verified KytyPS5 CMake build adapter for this repository".into());
    }
    let source = PathBuf::from(
        repository
            .source_path
            .ok_or("Clone this repository first")?,
    );
    if validate_clean_source_checkout(&source).is_err() {
        return Err("The tracked source checkout is missing or has local changes; community builds use a clean isolated clone".into());
    }
    let (owner, name, _) = parse_repo(&repository.url)?;
    let jobs = jobs.inner().clone();
    let job_id = begin_job(&jobs, "Another job is running", true, "community-build")?;
    thread::spawn(move || {
        let mut history_result_id: Option<i64> = None;
        let result = (|| -> Result<String, String> {
            let mut prs = Vec::new();
            let mut base_branch: Option<String> = None;
            for selection in &pull_requests {
                ensure_job_not_cancelled(&jobs)?;
                let number = selection.number;
                emit(
                    &app,
                    "community-build",
                    "Fetching PRs",
                    format!("Resolving {owner}/{name} PR #{number}"),
                    false,
                    true,
                );
                let api: GithubPullRequestApi = github_json(&format!(
                    "https://api.github.com/repos/{owner}/{name}/pulls/{number}"
                ))?;
                let pr = CommunityPullRequest::from(api);
                if pr.head_sha != selection.head_sha || pr.base_sha != selection.base_sha {
                    return Err(format!(
                        "PR #{} changed after review (reviewed head/base {}/{}, current {}/{}). Refresh and review the new commits before building.",
                        pr.number,
                        selection.head_sha,
                        selection.base_sha,
                        pr.head_sha,
                        pr.base_sha
                    ));
                }
                if pr
                    .base_repo
                    .as_deref()
                    .is_some_and(|base| !base.eq_ignore_ascii_case(&format!("{owner}/{name}")))
                {
                    return Err(format!("PR #{} targets a different repository", pr.number));
                }
                if let Some(branch) = &base_branch {
                    if branch != &pr.base_branch {
                        return Err(format!(
                            "PR #{} targets {}, while the other selected PRs target {}",
                            pr.number, pr.base_branch, branch
                        ));
                    }
                } else {
                    base_branch = Some(pr.base_branch.clone());
                }
                prs.push(pr);
            }
            ensure_job_not_cancelled(&jobs)?;
            let branch = base_branch.clone().unwrap_or_else(|| "unknown".into());
            let paths = state_paths(&app)?;
            let work_root = paths.cache.join("community-worktrees");
            if fs::symlink_metadata(&work_root)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                return Err("Community worktree cache cannot be a symbolic link".into());
            }
            fs::create_dir_all(&work_root).map_err(|e| e.to_string())?;
            let work_root = fs::canonicalize(&work_root).map_err(|error| error.to_string())?;
            let cache_root = fs::canonicalize(&paths.cache).map_err(|error| error.to_string())?;
            if !path_starts_with(&work_root, &cache_root) {
                return Err("Community worktree cache is outside Studio's managed cache".into());
            }
            // Windows canonicalize adds a verbatim \\?\ prefix that Git for Windows
            // rejects as a clone destination. Keep the canonical containment check
            // above, then use an ordinary absolute path for Git and build tools.
            let work_root = PathBuf::from(git_path_arg(&work_root)?);
            let worktree = work_root.join(format!(
                "repo-{repository_id}-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let _worktree_cleanup = ManagedTemporaryDirectory {
                path: worktree.clone(),
                root: work_root.clone(),
            };
            let source_text = git_path_arg(&source)?;
            let work_text = git_path_arg(&worktree)?;
            emit(
                &app,
                "community-build",
                "Preparing repository",
                "Creating an isolated Git clone; the registered source checkout remains untouched",
                false,
                true,
            );
            git_cancellable(
                &app,
                &jobs,
                None,
                &["clone", "--no-checkout", &source_text, &work_text],
            )?;
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["config", "core.hooksPath", "/dev/null"],
            )?;
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["config", "user.name", "KytyPS5 Studio"],
            )?;
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["config", "user.email", "studio@localhost"],
            )?;
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["remote", "set-url", "origin", &repository.url],
            )?;
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["check-ref-format", "--branch", &branch],
            )?;
            let base_ref = format!("+refs/heads/{branch}:refs/studio/base");
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["fetch", "--no-tags", "origin", &base_ref],
            )?;
            let base_sha = git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["rev-parse", "refs/studio/base"],
            )?;
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["checkout", "--detach", &base_sha],
            )?;
            let signature = community_build_signature(
                repository_id,
                &branch,
                &base_sha,
                &prs.iter()
                    .map(|pr| (pr.number, pr.head_sha.clone()))
                    .collect::<Vec<_>>(),
                &options.build_type,
                options.jobs,
                &compiler,
                options.clean,
            );
            let history_db = db(&app)?;
            history_db.execute("INSERT OR IGNORE INTO community_build_configs(signature,repo_id,name,base_branch,base_sha,build_type,jobs,created_at,compiler,clean) VALUES(?,?,?,?,?,?,?,?,?,?)", params![signature, repository_id, format!("Community PR Build — {}", pull_requests.iter().map(|selection| format!("#{}", selection.number)).collect::<Vec<_>>().join(" + ")), branch, base_sha, options.build_type, options.jobs, now(), compiler, options.clean]).map_err(|e| e.to_string())?;
            let config_id: i64 = history_db
                .query_row(
                    "SELECT id FROM community_build_configs WHERE signature=?",
                    [&signature],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            for (position, pr) in prs.iter().enumerate() {
                history_db.execute("INSERT OR IGNORE INTO community_build_prs(config_id,position,number,head_repo,head_sha) VALUES(?,?,?,?,?)", params![config_id, position as i64, pr.number as i64, pr.head_repo, pr.head_sha]).map_err(|e| e.to_string())?;
            }
            history_result_id = Some(history_db.query_row("INSERT INTO community_build_results(config_id,status,started_at) VALUES(?,'preparing',?) RETURNING id", params![config_id, now()], |row| row.get(0)).map_err(|e| e.to_string())?);
            for pr in &prs {
                ensure_job_not_cancelled(&jobs)?;
                let reference = format!("refs/studio/pr/{}", pr.number);
                let source_ref = format!("+refs/pull/{}/head:{}", pr.number, reference);
                emit(
                    &app,
                    "community-build",
                    "Applying PRs",
                    format!(
                        "Fetching and applying PR #{} at {}",
                        pr.number,
                        &pr.head_sha[..pr.head_sha.len().min(12)]
                    ),
                    false,
                    true,
                );
                git_cancellable(
                    &app,
                    &jobs,
                    Some(&worktree),
                    &["fetch", "--no-tags", "origin", &source_ref],
                )?;
                let fetched =
                    git_cancellable(&app, &jobs, Some(&worktree), &["rev-parse", &reference])?;
                if fetched != pr.head_sha {
                    return Err(format!(
                        "PR #{} head changed during fetch (API {}, fetched {})",
                        pr.number, pr.head_sha, fetched
                    ));
                }
                if let Err(error) = git_deterministic_merge(&app, &jobs, &worktree, &reference) {
                    let files = git_cancellable(
                        &app,
                        &jobs,
                        Some(&worktree),
                        &["diff", "--name-only", "--diff-filter=U"],
                    )
                    .unwrap_or_default();
                    return Err(format!("PR #{} could not be applied to this combination. Conflicting files: {}. Git: {}. Change the PR order or exclude a PR; Studio will not resolve source conflicts automatically.", pr.number, if files.is_empty() { "none reported" } else { &files }, error));
                }
            }
            ensure_job_not_cancelled(&jobs)?;
            let submodule_paths = git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &[
                    "config",
                    "--file",
                    ".gitmodules",
                    "--get-regexp",
                    r"\.path$",
                ],
            )
            .unwrap_or_default();
            for line in submodule_paths.lines() {
                let (_, path) = line
                    .split_once(' ')
                    .ok_or("Could not parse submodule path from .gitmodules")?;
                let candidate = Path::new(path);
                if candidate.is_absolute()
                    || candidate.components().any(|component| {
                        matches!(
                            component,
                            std::path::Component::ParentDir
                                | std::path::Component::RootDir
                                | std::path::Component::Prefix(_)
                        )
                    })
                {
                    return Err(format!("Unsafe submodule path in selected PRs: {path}"));
                }
            }
            let submodule_urls = git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["config", "--file", ".gitmodules", "--get-regexp", r"\.url$"],
            )
            .unwrap_or_default();
            for line in submodule_urls.lines() {
                let (_, url) = line
                    .split_once(' ')
                    .ok_or("Could not parse submodule URL from .gitmodules")?;
                parse_repo(url).map_err(|_| {
                    format!("Submodule URL is not an HTTPS GitHub repository: {url}")
                })?;
            }
            if !submodule_paths.trim().is_empty() {
                emit(
                    &app,
                    "community-build",
                    "Submodule update",
                    "Updating validated GitHub submodules in the isolated source clone",
                    false,
                    true,
                );
                git_cancellable(
                    &app,
                    &jobs,
                    Some(&worktree),
                    &["submodule", "update", "--init", "--recursive"],
                )?;
            }
            let final_commit =
                git_cancellable(&app, &jobs, Some(&worktree), &["rev-parse", "HEAD"])?;
            let result_id =
                history_result_id.ok_or("Community build history row was not created")?;
            db(&app)?.execute("UPDATE community_build_results SET final_commit=?,status='building' WHERE id=?", params![final_commit, result_id]).map_err(|e| e.to_string())?;
            emit(&app, "community-build", "Checking merge conflicts", format!("All {} selected PRs applied in the chosen order. Git merge success does not establish runtime or gameplay compatibility.", prs.len()), false, true);
            let build_options = BuildOptions {
                clean: options.clean,
                ..options.clone()
            };
            match build_and_register(
                &app,
                &jobs,
                repository_id,
                &worktree,
                &final_commit,
                &build_options,
                &compiler,
                "community-build",
                false,
            ) {
                Ok(install) => {
                    let connection = db(&app)?;
                    let build_id: i64 = connection
                        .query_row(
                            "SELECT id FROM builds WHERE repo_id=? AND commit_sha=? AND path=?",
                            params![repository_id, final_commit, install.display().to_string()],
                            |row| row.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    connection.execute("UPDATE community_build_results SET build_id=?,install_path=?,status='verified',finished_at=? WHERE id=?", params![build_id, install.display().to_string(), now(), result_id]).map_err(|e| e.to_string())?;
                    Ok(format!(
                        "Combined PR build verified and registered at {}",
                        install.display()
                    ))
                }
                Err(error) => {
                    if let Ok(connection) = db(&app) {
                        let _ = connection.execute("UPDATE community_build_results SET status='failed',finished_at=?,error=? WHERE id=?", params![now(), error, result_id]);
                    }
                    Err(error)
                }
            }
        })();
        if let (Some(result_id), Err(error)) = (history_result_id, &result) {
            if let Ok(connection) = db(&app) {
                let _ = connection.execute(
                    "UPDATE community_build_results SET status=?,finished_at=?,error=? WHERE id=?",
                    params![
                        if error.contains("cancel") {
                            "cancelled"
                        } else {
                            "failed"
                        },
                        now(),
                        error,
                        result_id
                    ],
                );
            }
        }
        finish_job(&app, &jobs, "community-build", result);
    });
    Ok(job_id)
}
#[derive(Serialize)]
struct CommunityBuildHistoryEntry {
    result_id: i64,
    config_id: i64,
    name: String,
    repository: String,
    base_branch: String,
    base_sha: String,
    build_type: String,
    jobs: u16,
    compiler: String,
    clean: bool,
    pull_requests: Vec<(u64, String)>,
    build_id: Option<i64>,
    install_path: Option<String>,
    status: String,
    started_at: i64,
    finished_at: Option<i64>,
    error: Option<String>,
    active: bool,
}
#[tauri::command]
fn community_build_history(
    app: tauri::AppHandle,
) -> Result<Vec<CommunityBuildHistoryEntry>, String> {
    let connection = db(&app)?;
    let mut statement = connection.prepare("SELECT result.id,config.id,config.name,repositories.owner || '/' || repositories.name,config.base_branch,config.base_sha,config.build_type,config.jobs,config.compiler,config.clean,result.build_id,result.install_path,result.status,result.started_at,result.finished_at,result.error,coalesce(builds.active,0) FROM community_build_results result JOIN community_build_configs config ON config.id=result.config_id JOIN repositories ON repositories.id=config.repo_id LEFT JOIN builds ON builds.id=result.build_id ORDER BY result.started_at DESC LIMIT 100").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(CommunityBuildHistoryEntry {
                result_id: row.get(0)?,
                config_id: row.get(1)?,
                name: row.get(2)?,
                repository: row.get(3)?,
                base_branch: row.get(4)?,
                base_sha: row.get(5)?,
                build_type: row.get(6)?,
                jobs: row.get::<_, i64>(7)?.clamp(1, 128) as u16,
                compiler: row.get(8)?,
                clean: row.get::<_, i64>(9)? != 0,
                pull_requests: Vec::new(),
                build_id: row.get(10)?,
                install_path: row.get(11)?,
                status: row.get(12)?,
                started_at: row.get(13)?,
                finished_at: row.get(14)?,
                error: row.get(15)?,
                active: row.get::<_, i64>(16)? != 0,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|error| error.to_string())?);
    }
    for entry in &mut entries {
        let mut prs = connection.prepare("SELECT number,head_sha FROM community_build_prs WHERE config_id=? ORDER BY position").map_err(|error| error.to_string())?;
        entry.pull_requests = prs
            .query_map([entry.config_id], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
    }
    Ok(entries)
}
#[tauri::command]
fn rename_community_build_config(
    app: tauri::AppHandle,
    config_id: i64,
    name: String,
) -> Result<(), String> {
    let name = name.trim();
    if config_id <= 0
        || name.is_empty()
        || name.chars().count() > 100
        || name.chars().any(char::is_control)
    {
        return Err("Choose a configuration name between 1 and 100 printable characters".into());
    }
    let changed = db(&app)?
        .execute(
            "UPDATE community_build_configs SET name=? WHERE id=?",
            params![name, config_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("Community build configuration was not found".into());
    }
    Ok(())
}
#[tauri::command]
fn discover_forks() -> Result<Vec<GithubRepo>, String> {
    github_json("https://api.github.com/repos/KytyPS5/KytyPS5/forks?per_page=30")
}
#[tauri::command]
fn github_repository_details(
    app: tauri::AppHandle,
    id: i64,
) -> Result<GithubRepositoryDetails, String> {
    let c = db(&app)?;
    let repository = repo(&c, id)?;
    let (owner, name, _) = parse_repo(&repository.url)?;
    let remote_url = format!("https://api.github.com/repos/{owner}/{name}");
    let remote: GithubRemoteRepo = github_json(&remote_url)?;
    let branch = if repository.branch.is_empty() {
        remote.default_branch.as_str()
    } else {
        repository.branch.as_str()
    };
    let sha_query = url::form_urlencoded::byte_serialize(branch.as_bytes()).collect::<String>();
    let commit_url =
        format!("https://api.github.com/repos/{owner}/{name}/commits?sha={sha_query}&per_page=1");
    let commits: Vec<GithubCommit> = github_json(&commit_url)?;
    let releases: Vec<GithubRelease> = github_json(&format!(
        "https://api.github.com/repos/{owner}/{name}/releases?per_page=10"
    ))?;
    Ok(GithubRepositoryDetails {
        default_branch: remote.default_branch,
        fork: remote.fork,
        stars: remote.stargazers_count,
        updated_at: remote.updated_at,
        latest_commit: commits.into_iter().next(),
        releases: releases
            .into_iter()
            .filter(|release| !release.draft)
            .collect(),
    })
}
#[tauri::command]
#[cfg(not(windows))]
fn system_info(app: tauri::AppHandle) -> Result<SystemInfo, String> {
    let p = state_paths(&app)?;
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.strip_prefix("model name\t: ")
                .or_else(|| line.strip_prefix("Hardware\t: "))
        })
        .unwrap_or("Unavailable")
        .trim()
        .to_string();
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let os = fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|s| s.strip_prefix("PRETTY_NAME="))
        .unwrap_or("Linux")
        .trim_matches('"')
        .to_string();
    let gpu = Command::new("lspci")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .find(|l| l.contains("VGA") || l.contains("3D controller"))
                .map(str::to_string)
        })
        .unwrap_or("Unavailable".into());
    let mut dependencies: Vec<Dependency> = [
        "git",
        "cmake",
        "ninja",
        "clang",
        "clang++",
        "ld.lld",
        "python3",
        "pkg-config",
        "glslangValidator",
        "vulkaninfo",
    ]
    .into_iter()
    .map(|n| Dependency {
        name: n.into(),
        present: if n == "glslangValidator" {
            any_command_available(&["glslangValidator", "glslang"], command_exists)
        } else {
            command_exists(n)
        },
    })
    .collect();
    for name in ["vulkan", "x11", "wayland-client", "libpulse", "libudev"] {
        dependencies.push(Dependency {
            name: name.into(),
            present: host_tool_command("pkg-config")
                .args(["--exists", name])
                .status()
                .map(|status| status.success())
                .unwrap_or(false),
        });
    }
    Ok(SystemInfo {
        os,
        arch: env::consts::ARCH.into(),
        cpu,
        memory_total_bytes: memory_value_bytes(&meminfo, "MemTotal").unwrap_or(0),
        memory_available_bytes: memory_value_bytes(&meminfo, "MemAvailable").unwrap_or(0),
        controllers: detected_controllers(
            &fs::read_to_string("/proc/bus/input/devices").unwrap_or_default(),
        ),
        session: env::var("XDG_SESSION_TYPE").unwrap_or("unknown".into()),
        gpu,
        dependencies,
        paths: HashMap::from([
            ("data".into(), p.data.display().to_string()),
            ("cache".into(), p.cache.display().to_string()),
            ("config".into(), p.config.display().to_string()),
        ]),
    })
}
#[tauri::command]
#[cfg(windows)]
async fn system_info(app: tauri::AppHandle) -> Result<SystemInfo, String> {
    tauri::async_runtime::spawn_blocking(move || system_info_sync(app))
        .await
        .map_err(|error| format!("System diagnostics task failed: {error}"))?
}
#[cfg(windows)]
fn system_info_sync(app: tauri::AppHandle) -> Result<SystemInfo, String> {
    let paths = state_paths(&app)?;
    let diagnostics = windows_diagnostics::collect()?;
    Ok(SystemInfo {
        os: diagnostics.os,
        arch: env::consts::ARCH.into(),
        cpu: diagnostics.cpu,
        memory_total_bytes: diagnostics.memory_total_bytes,
        memory_available_bytes: diagnostics.memory_available_bytes,
        controllers: diagnostics.controllers,
        session: diagnostics.session,
        gpu: diagnostics.gpu,
        dependencies: diagnostics
            .dependencies
            .into_iter()
            .map(|(name, present)| Dependency { name, present })
            .collect(),
        paths: HashMap::from([
            ("data".into(), paths.data.display().to_string()),
            ("cache".into(), paths.cache.display().to_string()),
            ("config".into(), paths.config.display().to_string()),
        ]),
    })
}
#[tauri::command]
fn inspect_graphics() -> GraphicsDiagnostics {
    graphics_diagnostics()
}
fn emit(
    app: &tauri::AppHandle,
    kind: &str,
    phase: &str,
    line: impl Into<String>,
    done: bool,
    success: bool,
) {
    let channel = job_event_channel(kind, phase);
    let _ = app.emit(
        "studio-job",
        JobEvent {
            job_id: if kind == "build"
                || kind == "clone"
                || kind == "community-build"
                || kind == "install-tools"
                || kind == "shader-precompile"
            {
                ACTIVE_JOB_ID.load(Ordering::SeqCst)
            } else {
                0
            },
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            channel: channel.into(),
            kind: kind.into(),
            phase: phase.into(),
            line: line.into(),
            done,
            success,
            progress: None,
        },
    );
}
fn job_event_channel(kind: &str, phase: &str) -> &'static str {
    match kind {
        "game" => "runtime",
        "shader-precompile" => "shader-precompile",
        "clone" => "repository-clone",
        "community-build" => "community-pr-build",
        "build"
            if matches!(
                phase,
                "Git" | "Fetching" | "Updating source" | "Updating submodules"
            ) =>
        {
            "repository-update"
        }
        "build" => "emulator-build",
        _ => "diagnostics",
    }
}
fn drain_job_output<R: Read>(
    stream: R,
    label: &str,
    app: &tauri::AppHandle,
    kind: &str,
    phase: &str,
    logfile: Option<&Arc<Mutex<BuildLogWriter>>>,
    capture: Option<&Arc<Mutex<Vec<String>>>>,
) {
    let mut reader = CappedOutputReader::new(stream);
    loop {
        match reader.next_line() {
            Ok(Some(output)) => {
                let line = output.text();
                if let Some(file) = logfile {
                    let _ = file.lock().unwrap().append_line(label, &line);
                }
                if let Some(capture) = capture {
                    let mut tail = capture.lock().unwrap();
                    if tail.len() == 30 {
                        tail.remove(0);
                    }
                    tail.push(line.chars().take(2048).collect());
                }
                emit(app, kind, phase, line, false, true);
            }
            Ok(None) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                let line = format!("output reader error: {error}");
                if let Some(file) = logfile {
                    let _ = file.lock().unwrap().append_line(label, &line);
                }
                emit(app, kind, phase, line, false, false);
                break;
            }
        }
    }
}
#[allow(clippy::too_many_arguments)] // Execution context is intentionally passed as separate borrowed inputs.
fn execute(
    app: &tauri::AppHandle,
    jobs: &Jobs,
    kind: &str,
    phase: &str,
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    log_path: Option<&Path>,
) -> Result<(), String> {
    if jobs.cancel_requested.load(Ordering::SeqCst) {
        return Err("Job cancelled".into());
    }
    let logfile: Option<Arc<Mutex<BuildLogWriter>>> = log_path
        .map(|path| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map(|file| Arc::new(Mutex::new(BuildLogWriter::new(file, BUILD_LOG_MAX_BYTES))))
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    if let Some(file) = &logfile {
        let _ = file
            .lock()
            .unwrap()
            .append_line("command", &format!("{program} {args:?}"));
    }
    emit(
        app,
        kind,
        phase,
        format!("$ {program} {args:?}"),
        false,
        true,
    );
    let mut c = host_tool_command(program);
    c.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    #[cfg(windows)]
    if program.eq_ignore_ascii_case("cmake") {
        windows_build::apply_environment(&mut c)?;
    }
    c.process_group(0);
    prepare_owned_child(&mut c);
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    attach_owned_child(&mut child)?;
    let pid = child.id() as i32;
    jobs.pid.store(pid, Ordering::SeqCst);
    if jobs.cancel_requested.load(Ordering::SeqCst) {
        let _ = stop_process_group(pid);
    }
    let stderr = child.stderr.take().ok_or("Missing stderr")?;
    let stderr_tail = Arc::new(Mutex::new(Vec::new()));
    let error_capture = stderr_tail.clone();
    let a = app.clone();
    let k = kind.to_string();
    let p = phase.to_string();
    let error_log = logfile.clone();
    let stderr_reader = thread::spawn(move || {
        drain_job_output(
            stderr,
            "stderr",
            &a,
            &k,
            &p,
            error_log.as_ref(),
            Some(&error_capture),
        );
    });
    let stdout = child.stdout.take().ok_or("Missing stdout")?;
    let a = app.clone();
    let k = kind.to_string();
    let p = phase.to_string();
    let output_log = logfile.clone();
    let stdout_reader = thread::spawn(move || {
        drain_job_output(stdout, "stdout", &a, &k, &p, output_log.as_ref(), None);
    });
    let waited = wait_for_child_and_cleanup_group(&mut child, || {
        jobs.pid.store(0, Ordering::SeqCst);
    });
    let stdout_result = stdout_reader
        .join()
        .map_err(|_| "Build stdout reader thread panicked".to_string());
    let stderr_result = stderr_reader
        .join()
        .map_err(|_| "Build stderr reader thread panicked".to_string());
    let (status, cleanup) = waited?;
    stdout_result?;
    stderr_result?;
    if let Err(error) = cleanup {
        if let Some(file) = &logfile {
            let _ = file.lock().unwrap().append_line("cleanup", &error);
        }
        return Err(format!("Could not clean up build process group: {error}"));
    }
    if jobs.cancel_requested.load(Ordering::SeqCst) {
        return Err("Job cancelled".into());
    }
    if !status.success() {
        let detail = stderr_tail.lock().unwrap().join("\n");
        return Err(if detail.is_empty() {
            format!("{} failed with {}", program, status)
        } else {
            format!("{} failed with {}: {}", program, status, detail)
        });
    }
    Ok(())
}
fn finish_job(app: &tauri::AppHandle, jobs: &Jobs, kind: &str, result: Result<String, String>) {
    let _control = jobs
        .control
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let cancel_requested = jobs.cancel_requested.swap(false, Ordering::SeqCst);
    let cancelled = result.is_err() && cancel_requested;
    jobs.pid.store(0, Ordering::SeqCst);
    jobs.cancellable.store(false, Ordering::SeqCst);
    jobs.active.store(false, Ordering::SeqCst);
    let (phase, line, success) = match result {
        Ok(message) => ("Complete", message, true),
        Err(_error) if cancelled => ("Cancelled", "Job cancelled".into(), false),
        Err(error) => ("Complete", error, false),
    };
    emit(app, kind, phase, line, true, success);
    ACTIVE_JOB_ID.store(0, Ordering::SeqCst);
    jobs.job_id.store(0, Ordering::SeqCst);
}
fn ensure_job_not_cancelled(jobs: &Jobs) -> Result<(), String> {
    if jobs.cancel_requested.load(Ordering::SeqCst) {
        Err("Job cancelled".into())
    } else {
        Ok(())
    }
}
fn source_dir(p: &Paths, id: i64) -> PathBuf {
    p.data.join("sources").join(id.to_string())
}
fn normalize_remote_url(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase()
}
fn restore_deleted_submodule_files(source: &Path) -> Result<(), String> {
    let parent_changes = git(
        Some(source),
        &[
            "status",
            "--porcelain",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ],
    )?;
    if !parent_changes.is_empty() {
        return Err("Existing checkout has local changes outside submodules; preserve or commit them before restoring".into());
    }
    let statuses = git(Some(source), &["submodule", "status", "--recursive"])?;
    for line in statuses.lines() {
        if line.starts_with('-') || line.starts_with('U') {
            continue;
        }
        let path = line
            .get(1..)
            .and_then(|value| value.split_whitespace().nth(1))
            .ok_or_else(|| format!("Could not parse submodule status: {line}"))?;
        let module = source.join(path);
        let changes = git(
            Some(&module),
            &["status", "--porcelain", "--untracked-files=normal"],
        )?;
        if changes.is_empty() {
            continue;
        }
        if changes.lines().any(|change| change.get(..2) != Some(" D")) {
            return Err(format!("Submodule {path} contains local edits or untracked files; preserve them before restoring"));
        }
        git(Some(&module), &["restore", "--worktree", "--", "."])?;
    }
    Ok(())
}
#[tauri::command]
fn start_clone(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    id: i64,
) -> Result<u64, String> {
    let c = db(&app)?;
    let r = repo(&c, id)?;
    let p = state_paths(&app)?;
    let target = source_dir(&p, id);
    let restoring = target.exists();
    let jobs = jobs.inner().clone();
    let job_id = begin_job(&jobs, "Another job is running", true, "clone")?;
    thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            if restoring {
                let actual = git(Some(&target), &["remote", "get-url", "origin"])?;
                if normalize_remote_url(&actual) != normalize_remote_url(&r.url) {
                    return Err(format!(
                        "Existing checkout at {} belongs to a different repository",
                        target.display()
                    ));
                }
                restore_deleted_submodule_files(&target)?;
                execute(
                    &app,
                    &jobs,
                    "clone",
                    "Restoring submodules",
                    "git",
                    &[
                        "submodule".into(),
                        "update".into(),
                        "--init".into(),
                        "--recursive".into(),
                    ],
                    Some(&target),
                    None,
                )?;
            } else {
                execute(
                    &app,
                    &jobs,
                    "clone",
                    "Cloning",
                    "git",
                    &[
                        "clone".into(),
                        "--recurse-submodules".into(),
                        r.url.clone(),
                        target.display().to_string(),
                    ],
                    None,
                    None,
                )?;
            }
            ensure_job_not_cancelled(&jobs)?;
            ensure_submodules_initialized(&target)?;
            let branch = checked_out_branch(&target)?;
            let adapter = inspect_adapter(&target);
            let c = db(&app)?;
            c.execute(
                "UPDATE repositories SET source_path=?,adapter=?,branch=? WHERE id=?",
                params![target.display().to_string(), adapter, branch, id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() && !restoring && target.exists() {
            let _ = fs::remove_dir_all(&target);
        }
        finish_job(
            &app,
            &jobs,
            "clone",
            result.map(|()| {
                if restoring {
                    "Checkout restored".into()
                } else {
                    "Repository ready".into()
                }
            }),
        );
    });
    Ok(job_id)
}
fn inspect_adapter(path: &Path) -> String {
    let cmake = fs::read_to_string(path.join("CMakeLists.txt")).unwrap_or_default();
    let main = fs::read_to_string(path.join("src/main.cpp")).unwrap_or_default();
    if cmake.contains("add_executable(kyty_emulator")
        && cmake.contains("KYTY_BUILD_LAUNCHER")
        && main.contains("--game")
    {
        "kyty-cmake".into()
    } else {
        "unknown".into()
    }
}
#[tauri::command]
fn inspect_repository(app: tauri::AppHandle, id: i64) -> Result<Repo, String> {
    let c = db(&app)?;
    let r = repo(&c, id)?;
    let path = r.source_path.as_ref().ok_or("Clone the repository first")?;
    let adapter = inspect_adapter(Path::new(path));
    c.execute(
        "UPDATE repositories SET adapter=? WHERE id=?",
        params![adapter, id],
    )
    .map_err(|e| e.to_string())?;
    repo(&c, id)
}
#[tauri::command]
async fn check_updates(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    id: i64,
) -> Result<String, String> {
    let jobs = jobs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || check_updates_sync(app, jobs, id))
        .await
        .map_err(|error| format!("Update check task failed: {error}"))?
}
fn check_updates_sync(app: tauri::AppHandle, jobs: Arc<Jobs>, id: i64) -> Result<String, String> {
    let _job_id = begin_job(
        &jobs,
        "A build or update job is already running",
        false,
        "build",
    )?;
    let result = (|| -> Result<String, String> {
        let c = db(&app)?;
        let r = repo(&c, id)?;
        let path = r.source_path.ok_or("Clone repository first")?;
        let p = Path::new(&path);
        let branch = checked_out_branch(p)?;
        if !r.branch.is_empty() && branch != r.branch {
            return Err(format!(
                "Checked out branch '{branch}' differs from tracked branch '{}'",
                r.branch
            ));
        }
        git(Some(p), &["fetch", "--prune", "origin"])?;
        let source_commit = git(Some(p), &["rev-parse", "HEAD"])?;
        let remote_ref = format!("refs/remotes/origin/{branch}^{{commit}}");
        let remote = git(Some(p), &["rev-parse", "--verify", &remote_ref])?;
        let installed: Option<String> = c
        .query_row(
            "SELECT commit_sha FROM builds WHERE repo_id=? ORDER BY active DESC,created_at DESC,id DESC LIMIT 1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
        Ok(update_status(&source_commit, installed.as_deref(), &remote))
    })();
    {
        let _control = jobs.control.lock().map_err(|error| error.to_string())?;
        jobs.cancellable.store(false, Ordering::SeqCst);
        jobs.active.store(false, Ordering::SeqCst);
        jobs.job_id.store(0, Ordering::SeqCst);
        ACTIVE_JOB_ID.store(0, Ordering::SeqCst);
    }
    result
}
fn update_status(source: &str, installed: Option<&str>, remote: &str) -> String {
    if source == remote && installed == Some(remote) {
        "Up to date".into()
    } else if source != remote && installed == Some(remote) {
        format!(
            "Installed build is current; source {} differs from tracked remote {}",
            &source[..source.len().min(12)],
            &remote[..remote.len().min(12)]
        )
    } else {
        let baseline = installed.unwrap_or("no verified build");
        let target = &remote[..remote.len().min(12)];
        let source_note = if source == remote {
            "; source is already at this commit"
        } else {
            ""
        };
        let baseline_short = if baseline == "no verified build" {
            baseline.to_string()
        } else {
            baseline[..baseline.len().min(12)].to_string()
        };
        format!("Update available: {baseline_short} → {target}{source_note}")
    }
}
#[derive(Clone, Deserialize)]
struct BuildOptions {
    build_type: String,
    jobs: u16,
    compiler: Option<String>,
    clean: bool,
}
#[derive(Serialize)]
struct BuildSourceInfo {
    branch: String,
    commit_sha: String,
    working_tree_status: Vec<String>,
    submodules: Vec<String>,
    build_directory: String,
    cmake_arguments: Vec<String>,
}
#[tauri::command]
async fn inspect_build_source(
    app: tauri::AppHandle,
    repository_id: i64,
    build_type: String,
) -> Result<BuildSourceInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspect_build_source_sync(app, repository_id, build_type)
    })
    .await
    .map_err(|error| format!("Build source inspection task failed: {error}"))?
}
fn inspect_build_source_sync(
    app: tauri::AppHandle,
    repository_id: i64,
    build_type: String,
) -> Result<BuildSourceInfo, String> {
    if !["Release", "Debug", "RelWithDebInfo"].contains(&build_type.as_str()) {
        return Err("Unsupported CMake build type".into());
    }
    let connection = db(&app)?;
    let repository = repo(&connection, repository_id)?;
    let source_path = PathBuf::from(
        repository
            .source_path
            .ok_or("Clone the selected repository first")?,
    );
    if !source_path.is_dir() {
        return Err("The repository source directory is missing".into());
    }
    let commit_sha = git(Some(&source_path), &["rev-parse", "HEAD"])?;
    let branch = git(Some(&source_path), &["branch", "--show-current"])?;
    let working_tree_status = git(
        Some(&source_path),
        &["status", "--short", "--untracked-files=normal"],
    )?
    .lines()
    .map(str::to_string)
    .collect::<Vec<_>>();
    let submodules = git(Some(&source_path), &["submodule", "status", "--recursive"])?
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let compiler = default_build_compiler();
    let cache_dir = state_paths(&app)?
        .cache
        .join("builds")
        .join(repository_id.to_string())
        .join(format!("{commit_sha}-{build_type}-{compiler}-emulator"));
    let cmake_arguments = displayed_cmake_arguments(&build_type)?;
    Ok(BuildSourceInfo {
        branch: if branch.is_empty() {
            format!(
                "Detached HEAD at {}",
                &commit_sha[..commit_sha.len().min(12)]
            )
        } else {
            branch
        },
        commit_sha,
        working_tree_status,
        submodules,
        build_directory: cache_dir.display().to_string(),
        cmake_arguments,
    })
}
fn validate_build_options(options: &BuildOptions) -> Result<String, String> {
    if !["Release", "Debug", "RelWithDebInfo"].contains(&options.build_type.as_str())
        || options.jobs == 0
        || options.jobs > 128
    {
        return Err("Invalid build options".into());
    }
    let compiler = options
        .compiler
        .clone()
        .unwrap_or_else(|| default_build_compiler().into());
    #[cfg(windows)]
    return match compiler.as_str() {
        "clang" | "clang-cl" => Ok("clang-cl".into()),
        _ => Err(
            "Windows emulator builds require clang-cl with the Visual Studio x64 toolchain".into(),
        ),
    };
    #[cfg(not(windows))]
    match compiler.as_str() {
        "clang" => Ok(compiler),
        _ => Err("Linux emulator builds require clang".into()),
    }
}
fn default_build_compiler() -> &'static str {
    default_build_compiler_for(cfg!(windows))
}
fn default_build_compiler_for(windows: bool) -> &'static str {
    if windows {
        "clang-cl"
    } else {
        "clang"
    }
}
fn cmake_configuration_arguments(build_type: &str) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        windows_build::cmake_arguments(build_type)
    }
    #[cfg(not(windows))]
    {
        let compiler = default_build_compiler();
        let mut arguments = vec![
            "-G".into(),
            "Ninja".into(),
            format!("-DCMAKE_BUILD_TYPE={build_type}"),
            format!("-DCMAKE_C_COMPILER={compiler}"),
            format!("-DCMAKE_CXX_COMPILER={compiler}++"),
        ];
        arguments.extend(linux_x86_64_cmake_cpu_flags(
            std::env::consts::OS,
            std::env::consts::ARCH,
        ));
        Ok(arguments)
    }
}
fn displayed_cmake_arguments(build_type: &str) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        windows_build::display_cmake_arguments(build_type)
    }
    #[cfg(not(windows))]
    {
        let arguments = cmake_configuration_arguments(build_type)?;
        let mut displayed = Vec::new();
        let mut iter = arguments.into_iter();
        while let Some(argument) = iter.next() {
            if argument == "-G" {
                displayed.push(format!("-G {}", iter.next().unwrap_or_default()));
            } else {
                displayed.push(argument);
            }
        }
        Ok(displayed)
    }
}
#[cfg(any(not(windows), test))]
const BUILD_DEPENDENCIES: &[&str] = &[
    "git",
    "cmake",
    "ninja",
    "clang",
    "clang++",
    "pkg-config",
    "python3",
];
#[cfg(any(not(windows), test))]
const BUILD_PKG_CONFIG_DEPENDENCIES: &[&str] =
    &["vulkan", "x11", "wayland-client", "libpulse", "libudev"];
#[cfg(any(not(windows), test))]
fn missing_build_dependencies<F>(mut available: F) -> Vec<&'static str>
where
    F: FnMut(&str) -> bool,
{
    BUILD_DEPENDENCIES
        .iter()
        .copied()
        .filter(|name| !available(name))
        .collect()
}
#[cfg(any(not(windows), test))]
fn missing_pkg_config_dependencies<F>(mut available: F) -> Vec<&'static str>
where
    F: FnMut(&str) -> bool,
{
    BUILD_PKG_CONFIG_DEPENDENCIES
        .iter()
        .copied()
        .filter(|name| !available(name))
        .collect()
}
#[cfg(any(not(windows), test))]
fn linux_x86_64_cmake_cpu_flags(target_os: &str, target_arch: &str) -> Vec<String> {
    if target_os == "linux" && target_arch == "x86_64" {
        vec!["-DCMAKE_CXX_FLAGS=-msse4.1".into()]
    } else {
        Vec::new()
    }
}
// Recent upstream Kyty sources include Microsoft's <intrin.h> unconditionally
// in bufferCache.cpp. On Linux, Clang's compatibility header forwards to a
// nonexistent system header. Put a tiny x86 compatibility header ahead of
// Clang's builtin include directory, without modifying the user's checkout.
#[cfg(not(windows))]
fn linux_x86_64_intrin_compat_flags(
    target_os: &str,
    target_arch: &str,
    source: &Path,
    build_dir: &Path,
) -> Result<Vec<String>, String> {
    if target_os != "linux" || target_arch != "x86_64" {
        return Ok(Vec::new());
    }
    let affected_source = source.join("src/graphics/host_gpu/renderer/cache/bufferCache.cpp");
    let contents = match fs::read_to_string(affected_source) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    if !contents
        .lines()
        .any(|line| line.trim() == "#include <intrin.h>")
    {
        return Ok(Vec::new());
    }

    let include_dir = build_dir.join("compat/include");
    fs::create_dir_all(&include_dir).map_err(|error| error.to_string())?;
    fs::write(include_dir.join("intrin.h"), "#include <x86intrin.h>\n")
        .map_err(|error| error.to_string())?;
    // CMake tokenizes CMAKE_CXX_FLAGS when generating compiler commands, so
    // quote the include path to preserve source/cache paths containing spaces.
    Ok(vec![format!(
        "-DCMAKE_CXX_FLAGS=-msse4.1 -I\"{}\"",
        include_dir.display()
    )])
}
fn verify_build_dependencies(app: &tauri::AppHandle, job_kind: &str) -> Result<(), String> {
    emit(
        app,
        job_kind,
        "Dependency verification",
        if cfg!(windows) {
            "Checking Windows emulator build tools"
        } else {
            "Checking Linux emulator build tools"
        },
        false,
        true,
    );
    #[cfg(windows)]
    {
        windows_build::validate()
    }
    #[cfg(not(windows))]
    {
        let mut missing = missing_build_dependencies(command_exists);
        if !any_command_available(&["glslangValidator", "glslang"], command_exists) {
            missing.push("glslangValidator or glslang");
        }
        let missing_packages = if command_exists("pkg-config") {
            missing_pkg_config_dependencies(|name| {
                host_tool_command("pkg-config")
                    .args(["--exists", name])
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false)
            })
        } else {
            Vec::new()
        };
        if missing.is_empty() && missing_packages.is_empty() {
            Ok(())
        } else {
            let mut issues = missing;
            issues.extend(missing_packages.into_iter().map(|name| match name {
                "vulkan" => "Vulkan development files",
                "x11" => "X11 development files",
                "wayland-client" => "Wayland development files",
                "libpulse" => "PulseAudio development files",
                "libudev" => "libudev development files",
                _ => name,
            }));
            Err(format!(
            "Missing build dependencies: {}. Install these with your distribution package manager, then retry.",
            issues.join(", ")
        ))
        }
    }
}
#[cfg(target_os = "linux")]
fn linux_build_tool_install_command() -> Result<(String, Vec<String>), String> {
    let packages = if command_exists("apt-get") {
        (
            "apt-get",
            vec![
                "git",
                "build-essential",
                "clang",
                "cmake",
                "ninja-build",
                "pkg-config",
                "python3",
                "glslang-tools",
                "libvulkan-dev",
                "libx11-dev",
                "libwayland-dev",
                "libpulse-dev",
                "libudev-dev",
            ],
        )
    } else if command_exists("dnf") {
        (
            "dnf",
            vec![
                "git",
                "gcc",
                "gcc-c++",
                "clang",
                "cmake",
                "ninja-build",
                "pkgconf-pkg-config",
                "python3",
                "glslang",
                "vulkan-headers",
                "vulkan-loader-devel",
                "libX11-devel",
                "wayland-devel",
                "pulseaudio-libs-devel",
                "systemd-devel",
            ],
        )
    } else if command_exists("pacman") {
        (
            "pacman",
            vec![
                "git",
                "base-devel",
                "clang",
                "cmake",
                "ninja",
                "pkgconf",
                "python",
                "glslang",
                "vulkan-headers",
                "libx11",
                "wayland",
                "libpulse",
                "systemd",
            ],
        )
    } else if command_exists("zypper") {
        (
            "zypper",
            vec![
                "git-core",
                "gcc",
                "gcc-c++",
                "clang",
                "cmake",
                "ninja",
                "pkg-config",
                "python3",
                "glslang",
                "vulkan-headers",
                "vulkan-loader-devel",
                "libX11-devel",
                "wayland-devel",
                "libpulse-devel",
                "systemd-devel",
            ],
        )
    } else {
        return Err("Automatic Linux tool installation supports apt, dnf, pacman, and zypper. Install the C++ build tools and Vulkan/X11/Wayland/PulseAudio/libudev development packages with your distribution package manager.".into());
    };
    let (manager, packages) = packages;
    if !command_exists("pkexec") {
        return Err("A graphical administrator prompt (pkexec) is unavailable. Install polkit, sign in to an administrator account, and try again.".into());
    }
    let mut args = vec![manager.to_string()];
    match manager {
        "apt-get" => args.extend(["install".into(), "-y".into()]),
        "dnf" => args.extend(["install".into(), "-y".into()]),
        "pacman" => args.extend(["-S".into(), "--needed".into(), "--noconfirm".into()]),
        "zypper" => args.extend(["--non-interactive".into(), "install".into()]),
        _ => unreachable!(),
    }
    args.extend(packages.into_iter().map(str::to_string));
    Ok(("pkexec".into(), args))
}

#[tauri::command]
async fn start_install_build_tools(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
) -> Result<u64, String> {
    let jobs = jobs.inner().clone();
    let job_id = begin_job(&jobs, "Another job is running", false, "install-tools")?;
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            emit(
                &app,
                "install-tools",
                "Installing tools",
                "Checking missing tools and preparing the system installer",
                false,
                true,
            );
            #[cfg(windows)]
            let commands = windows_build::install_commands()?;
            #[cfg(target_os = "linux")]
            let commands = [linux_build_tool_install_command()?];
            #[cfg(not(any(windows, target_os = "linux")))]
            return Err(
                "Automatic build tool installation is supported on Windows and Linux only.".into(),
            );
            for (index, (program, args)) in commands.iter().enumerate() {
                emit(
                    &app,
                    "install-tools",
                    "Installing tools",
                    format!("Installing package set {}/{}", index + 1, commands.len()),
                    false,
                    true,
                );
                let install_result = execute(
                    &app,
                    &jobs,
                    "install-tools",
                    "Installing tools",
                    program,
                    args,
                    None,
                    None,
                );
                if let Err(error) = install_result {
                    #[cfg(windows)]
                    let already_satisfied = {
                        let package_id = args
                            .windows(2)
                            .find(|pair| pair[0] == "--id")
                            .map(|pair| pair[1].as_str());
                        error.to_ascii_lowercase().contains("0x8a15002b")
                            && package_id
                                .is_some_and(windows_build::package_requirement_is_satisfied)
                    };
                    #[cfg(not(windows))]
                    let already_satisfied = false;
                    if already_satisfied {
                        emit(
                            &app,
                            "install-tools",
                            "Installing tools",
                            "WinGet reports no applicable update; the required tool is already installed",
                            false,
                            true,
                        );
                    } else {
                        return Err(error);
                    }
                }
            }
            #[cfg(windows)]
            let message = "Tool installation finished. Restart KytyPS5 Studio so Windows can refresh PATH and Vulkan SDK settings, then check dependencies again.";
            #[cfg(target_os = "linux")]
            let message =
                "Tool installation finished. Return to Build Studio and check dependencies again.";
            Ok(message.into())
        })();
        finish_job(&app, &jobs, "install-tools", result);
    });
    Ok(job_id)
}
fn validate_clean_source_checkout(source: &Path) -> Result<(), String> {
    validate_source_and_submodule_worktrees_clean(source)?;
    ensure_submodules_initialized(source)?;
    Ok(())
}
fn ensure_submodules_initialized(source: &Path) -> Result<(), String> {
    let submodules = git(Some(source), &["submodule", "status", "--recursive"])?;
    if submodules
        .lines()
        .any(|line| line.starts_with('-') || line.starts_with('+') || line.starts_with('U'))
    {
        return Err("Checkout is incomplete: one or more Git submodules are uninitialized or differ from the recorded commit. Restore the checkout again before building.".into());
    }
    Ok(())
}

// This preflight runs before the build's submodule update. It permits missing
// or stale submodule checkouts so that the update step can repair them, while
// still protecting local edits in the main repository and initialized modules.
fn validate_source_before_submodule_update(source: &Path) -> Result<(), String> {
    validate_source_and_submodule_worktrees_clean(source)
}

fn validate_source_and_submodule_worktrees_clean(source: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err("The registered source checkout is missing".into());
    }
    if !git(
        Some(source),
        &[
            "status",
            "--porcelain",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ],
    )?
    .is_empty()
    {
        return Err("Source checkout has modified or untracked files. Commit or remove them before building so the recorded commit matches the built source.".into());
    }
    let dirty_submodules = git(
        Some(source),
        &[
            "submodule",
            "foreach",
            "--quiet",
            "--recursive",
            "git status --porcelain --untracked-files=normal",
        ],
    )?;
    if !dirty_submodules.trim().is_empty() {
        return Err("One or more Git submodules contain modified or untracked files. Clean them before building so the recorded commit matches the built source.".into());
    }
    Ok(())
}
struct IncompleteInstall {
    path: PathBuf,
    keep: bool,
}
impl Drop for IncompleteInstall {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
#[allow(clippy::too_many_arguments)] // Database columns remain explicit at this single insertion boundary.
fn insert_registered_build(
    connection: &Connection,
    repository_id: i64,
    commit: &str,
    build_type: &str,
    path: &Path,
    created_at: i64,
    install_size_bytes: Option<u64>,
    configuration_json: &str,
) -> Result<bool, String> {
    let inserted = connection
        .execute(
            "INSERT OR IGNORE INTO builds(repo_id,commit_sha,build_type,path,created_at,install_size_bytes,validation_status,configuration_json) VALUES(?,?,?,?,?,?,'verified',?)",
            params![repository_id, commit, build_type, path.display().to_string(), created_at, install_size_bytes.map(|size| size.min(i64::MAX as u64) as i64), configuration_json],
        )
        .map_err(|error| error.to_string())?;
    Ok(inserted == 1)
}
fn build_optional_shader_tool(
    app: &tauri::AppHandle,
    jobs: &Jobs,
    kind: &str,
    build_dir: &Path,
    log_path: &Path,
    target: &str,
) {
    let mut help = host_tool_command("cmake");
    help.arg("--build")
        .arg(build_dir)
        .args(["--target", "help"]);
    #[cfg(windows)]
    if windows_build::apply_environment(&mut help).is_err() {
        return;
    }
    let Ok(output) = help.output() else { return };
    if !output.status.success() {
        return;
    }
    let targets = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !targets.lines().any(|line| line.contains(target)) {
        return;
    }
    if let Err(error) = execute(
        app,
        jobs,
        kind,
        "Shader tools",
        "cmake",
        &[
            "--build".into(),
            build_dir.display().to_string(),
            "--target".into(),
            target.into(),
            "--parallel".into(),
            "2".into(),
        ],
        None,
        Some(log_path),
    ) {
        if let Ok(mut log) = fs::OpenOptions::new().append(true).open(log_path) {
            let _ = writeln!(
                log,
                "[shader-tools] optional target {target} was unavailable: {error}"
            );
        }
    }
}

fn copy_directory_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

fn precompile_tool_candidates(root: &Path) -> Vec<PathBuf> {
    let platform = if cfg!(windows) { "windows" } else { "linux" };
    let names: &[&str] = if cfg!(windows) {
        &["kyty_shader_precompile.exe"]
    } else {
        &["kyty_shader_precompile", "kyty_shader_precompile.exe"]
    };
    names
        .iter()
        .flat_map(|name| {
            [
                root.join(name),
                root.join("Release").join(name),
                root.join("_Build").join(platform).join(name),
                root.join("tools/local/static-precompile").join(name),
            ]
        })
        .collect()
}

fn shader_seeds_tool_candidates(root: &Path) -> Vec<PathBuf> {
    let platform = if cfg!(windows) { "windows" } else { "linux" };
    let names: &[&str] = if cfg!(windows) {
        &["kyty_seeds.exe"]
    } else {
        &["kyty_seeds", "kyty_seeds.exe"]
    };
    names
        .iter()
        .flat_map(|name| {
            [
                root.join(name),
                root.join("Release").join(name),
                root.join("_Build/seeds-dist").join(name),
                root.join("_Build").join(platform).join(name),
                root.join("tools/local/static-precompile").join(name),
            ]
        })
        .collect()
}

fn shader_precompile_tool_available(path: &Path) -> bool {
    if !cfg!(windows)
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        path.is_file()
    } else {
        is_emulator_executable(path)
    }
}

fn stage_shader_precompile_assets(
    source: &Path,
    build_dir: &Path,
    install: &Path,
) -> Result<(), String> {
    let Some(script) = [build_dir, source, install]
        .into_iter()
        .flat_map(precompile_script_candidates)
        .find(|path| path.is_file())
    else {
        return Ok(());
    };
    let script_requires_windows_tool = precompile_script_requires_windows_tool(&script);
    let Some(tool) = precompile_tool_candidates(build_dir)
        .into_iter()
        .chain(precompile_tool_candidates(source))
        .chain(precompile_tool_candidates(install))
        .find(|path| {
            if script_requires_windows_tool {
                path.file_name()
                    .is_some_and(|name| name == "kyty_shader_precompile.exe")
                    && path.is_file()
            } else {
                shader_precompile_tool_available(path)
            }
        })
    else {
        return Ok(());
    };
    let script_name = script
        .file_name()
        .ok_or("Shader precompile script has no filename")?;
    fs::copy(&script, install.join(script_name)).map_err(|error| error.to_string())?;
    let tool_name = tool
        .file_name()
        .ok_or("Shader precompile tool has no filename")?;
    if tool != install.join(tool_name) {
        fs::copy(&tool, install.join(tool_name)).map_err(|error| error.to_string())?;
    }
    if let Some(seeds) = [build_dir, source, install]
        .into_iter()
        .flat_map(shader_seeds_tool_candidates)
        .find(|path| shader_precompile_tool_available(path))
    {
        let seeds_name = seeds
            .file_name()
            .ok_or("Shader seeds tool has no filename")?
            .to_owned();
        let staged_seeds = install.join(seeds_name);
        if seeds != staged_seeds {
            fs::copy(&seeds, staged_seeds).map_err(|error| error.to_string())?;
        }
    }
    for root in [source, build_dir] {
        let support_files = root.join("tools/local/static-precompile");
        if support_files.is_dir() {
            copy_directory_tree(
                &support_files,
                &install.join("tools/local/static-precompile"),
            )
            .map_err(|error| error.to_string())?;
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Build orchestration uses explicit inputs shared by both host platforms.
fn build_and_register(
    app: &tauri::AppHandle,
    jobs: &Jobs,
    repository_id: i64,
    source: &Path,
    commit: &str,
    options: &BuildOptions,
    compiler: &str,
    job_kind: &str,
    reuse_repository_build: bool,
) -> Result<PathBuf, String> {
    ensure_job_not_cancelled(jobs)?;
    validate_clean_source_checkout(source)?;
    verify_build_dependencies(app, job_kind)?;
    let paths = state_paths(app)?;
    let build_dir = paths
        .cache
        .join("builds")
        .join(repository_id.to_string())
        .join(format!(
            "{commit}-{}-{compiler}-emulator",
            options.build_type
        ));
    let install = paths
        .data
        .join("builds")
        .join(repository_id.to_string())
        .join(format!(
            "{}-{}-{}",
            &commit[..commit.len().min(12)],
            options.build_type,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
    if install.exists() {
        return Err("Refusing to overwrite an existing emulator installation".into());
    }
    let mut install_guard = IncompleteInstall {
        path: install.clone(),
        keep: false,
    };
    if build_dir.exists() && options.clean {
        fs::remove_dir_all(&build_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;
    let build_dir_s = build_dir.display().to_string();
    let source_s = source.display().to_string();
    let mut conf = vec!["-S".into(), source_s, "-B".into(), build_dir_s.clone()];
    conf.extend(cmake_configuration_arguments(&options.build_type)?);
    #[cfg(not(windows))]
    conf.extend(linux_x86_64_intrin_compat_flags(
        std::env::consts::OS,
        std::env::consts::ARCH,
        source,
        &build_dir,
    )?);
    conf.push("-DKYTY_BUILD_LAUNCHER=OFF".into());
    let log_path = build_dir.join("build.log");
    fs::write(&log_path, b"").map_err(|e| e.to_string())?;
    execute(
        app,
        jobs,
        job_kind,
        "Configure",
        "cmake",
        &conf,
        None,
        Some(&log_path),
    )?;
    execute(
        app,
        jobs,
        job_kind,
        "Compile",
        "cmake",
        &[
            "--build".into(),
            build_dir_s.clone(),
            "--target".into(),
            "kyty_emulator".into(),
            "--parallel".into(),
            options.jobs.to_string(),
        ],
        None,
        Some(&log_path),
    )?;
    build_optional_shader_tool(
        app,
        jobs,
        job_kind,
        &build_dir,
        &log_path,
        "kyty_shader_precompile",
    );
    build_optional_shader_tool(app, jobs, job_kind, &build_dir, &log_path, "kyty_seeds");
    execute(
        app,
        jobs,
        job_kind,
        "Install",
        "cmake",
        &[
            "--install".into(),
            build_dir_s,
            "--prefix".into(),
            install.display().to_string(),
        ],
        None,
        Some(&log_path),
    )?;
    ensure_job_not_cancelled(jobs)?;
    if let Err(error) = stage_shader_precompile_assets(source, &build_dir, &install) {
        if let Ok(mut log) = fs::OpenOptions::new().append(true).open(&log_path) {
            let _ = writeln!(
                log,
                "[shader-tools] could not stage optional precompile assets: {error}"
            );
        }
    }
    emit(
        app,
        job_kind,
        "Validation",
        "Checking the installed emulator executable and game CLI support",
        false,
        true,
    );
    verify_emulator(&install.join(emulator_binary_name()))?;
    ensure_job_not_cancelled(jobs)?;
    fs::copy(&log_path, install.join("build.log")).map_err(|e| e.to_string())?;
    let install_size_bytes = directory_size_bytes(&install);
    let configuration_json = serde_json::to_string(&BuildConfiguration {
        compiler: Some(compiler.to_string()),
        jobs: Some(options.jobs),
        clean: Some(options.clean),
        cmake_arguments: conf.clone(),
    })
    .map_err(|error| error.to_string())?;
    ensure_job_not_cancelled(jobs)?;
    let connection = paths.db()?;
    let replaced_install = if reuse_repository_build {
        let existing: Option<(i64, String)> = connection
            .query_row(
                "SELECT id,path FROM builds WHERE repo_id=? AND build_type=? AND NOT EXISTS(SELECT 1 FROM community_build_results WHERE community_build_results.build_id=builds.id) ORDER BY active DESC,created_at DESC,id DESC LIMIT 1",
                params![repository_id, options.build_type],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some((existing_id, existing_path)) = existing {
            let running: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sessions WHERE build_id=? AND ended_at IS NULL)",
                    [existing_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if running {
                return Err(
                    "Stop the running emulator session before rebuilding this repository.".into(),
                );
            }
            let changed = connection
                .execute(
                    "UPDATE builds SET commit_sha=?,path=?,created_at=?,install_size_bytes=?,validation_status='verified',configuration_json=?,removed_at=NULL WHERE id=?",
                    params![commit, install.display().to_string(), now(), install_size_bytes.map(|size| size.min(i64::MAX as u64) as i64), configuration_json, existing_id],
                )
                .map_err(|error| error.to_string())?;
            if changed != 1 {
                return Err("Existing emulator build changed while it was being rebuilt.".into());
            }
            Some(PathBuf::from(existing_path))
        } else {
            if !insert_registered_build(
                &connection,
                repository_id,
                commit,
                &options.build_type,
                &install,
                now(),
                install_size_bytes,
                &configuration_json,
            )? {
                return Err(
                    "Build output was not registered because its installation path already exists"
                        .into(),
                );
            }
            None
        }
    } else {
        if !insert_registered_build(
            &connection,
            repository_id,
            commit,
            &options.build_type,
            &install,
            now(),
            install_size_bytes,
            &configuration_json,
        )? {
            return Err(
                "Build output was not registered because its installation path already exists"
                    .into(),
            );
        }
        None
    };
    install_guard.keep = true;
    if let Some(previous) = replaced_install {
        remove_replaced_install(&previous, &install, &paths.data.join("builds"));
    }
    Ok(install)
}
fn remove_replaced_install(previous: &Path, replacement: &Path, managed_root: &Path) {
    if previous == replacement || !previous.exists() {
        return;
    }
    let Ok(root) = fs::canonicalize(managed_root) else {
        return;
    };
    let Ok(metadata) = fs::symlink_metadata(previous) else {
        return;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return;
    }
    let Ok(canonical) = fs::canonicalize(previous) else {
        return;
    };
    if !paths_equal(&canonical, &root)
        && path_starts_with(&canonical, &root)
        && !paths_equal(&canonical, replacement)
    {
        let _ = fs::remove_dir_all(canonical);
    }
}
fn directory_size_bytes(path: &Path) -> Option<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.ok()?;
        if entry.file_type().is_file() {
            total = total.checked_add(entry.metadata().ok()?.len())?;
        }
    }
    Some(total)
}
fn mark_repository_build_commit(
    app: &tauri::AppHandle,
    repository_id: i64,
    commit: &str,
    branch: &str,
) -> Result<(), String> {
    state_paths(app)?
        .db()?
        .execute(
            "UPDATE repositories SET installed_commit=?,branch=? WHERE id=?",
            params![commit, branch, repository_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
#[tauri::command]
fn start_build(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    id: i64,
    options: BuildOptions,
    activate_after_build: Option<bool>,
) -> Result<u64, String> {
    let compiler = validate_build_options(&options)?;
    let c = db(&app)?;
    let r = repo(&c, id)?;
    if !["official", "kyty-cmake"].contains(&r.adapter.as_str()) {
        return Err("No verified build adapter for this checkout".into());
    }
    let source = PathBuf::from(r.source_path.ok_or("Clone the repository first")?);
    validate_source_before_submodule_update(&source)?;
    let commit = git(Some(&source), &["rev-parse", "HEAD"])?;
    let branch = checked_out_branch(&source)?;
    let activate_after_build = activate_after_build.unwrap_or(false);
    let jobs = jobs.inner().clone();
    let job_id = begin_job(&jobs, "Another job is running", true, "build")?;
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            if !options.clean {
                let connection = db(&app)?;
                let existing: Option<(i64, String)> = connection
                    .query_row(
                        "SELECT builds.id,builds.path FROM builds WHERE builds.repo_id=? AND builds.commit_sha=? AND builds.build_type=? AND builds.validation_status='verified' AND NOT EXISTS(SELECT 1 FROM community_build_results WHERE community_build_results.build_id=builds.id) ORDER BY builds.active DESC,builds.created_at DESC,builds.id DESC LIMIT 1",
                        params![id, commit, options.build_type],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                if let Some((existing_id, existing_path)) = existing {
                    if verify_emulator(&Path::new(&existing_path).join(emulator_binary_name()))
                        .is_ok()
                    {
                        mark_repository_build_commit(&app, id, &commit, &branch)?;
                        if activate_after_build {
                            activate_build(app.clone(), existing_id)?;
                        }
                        return Ok(format!(
                            "Commit {} is already installed and verified; reused the existing build{}",
                            &commit[..commit.len().min(12)],
                            if activate_after_build { " and activated it" } else { "" }
                        ));
                    }
                }
            }
            execute(
                &app,
                &jobs,
                "build",
                "Updating submodules",
                "git",
                &[
                    "submodule".into(),
                    "update".into(),
                    "--init".into(),
                    "--recursive".into(),
                ],
                Some(&source),
                None,
            )?;
            let path = build_and_register(
                &app, &jobs, id, &source, &commit, &options, &compiler, "build", true,
            )?;
            mark_repository_build_commit(&app, id, &commit, &branch)?;
            if activate_after_build {
                let connection = db(&app)?;
                let build_id: i64 = connection
                    .query_row(
                        "SELECT id FROM builds WHERE path=?",
                        [path.display().to_string()],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                activate_build(app.clone(), build_id)?;
            }
            Ok(format!(
                "Build verified and registered at {}{}",
                path.display(),
                if activate_after_build {
                    "; now active for game launches"
                } else {
                    ""
                }
            ))
        })();
        finish_job(&app, &jobs, "build", result);
    });
    Ok(job_id)
}
#[tauri::command]
fn start_update_build(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    id: i64,
    options: BuildOptions,
    activate_after_build: Option<bool>,
) -> Result<u64, String> {
    let compiler = validate_build_options(&options)?;
    let c = db(&app)?;
    let repository = repo(&c, id)?;
    if !["official", "kyty-cmake"].contains(&repository.adapter.as_str()) {
        return Err("No verified build adapter for this checkout".into());
    }
    let source = PathBuf::from(repository.source_path.ok_or("Clone the repository first")?);
    let activate_after_build = activate_after_build.unwrap_or(false);
    let jobs = jobs.inner().clone();
    let job_id = begin_job(&jobs, "Another job is running", true, "build")?;
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let branch = checked_out_branch(&source)?;
            if !repository.branch.is_empty() && branch != repository.branch {
                return Err(format!(
                    "Checked out branch '{branch}' differs from tracked branch '{}'",
                    repository.branch
                ));
            }
            validate_source_before_submodule_update(&source)?;
            execute(
                &app,
                &jobs,
                "build",
                "Fetching",
                "git",
                &["fetch".into(), "--prune".into(), "origin".into()],
                Some(&source),
                None,
            )?;
            let remote_ref = format!("refs/remotes/origin/{branch}^{{commit}}");
            let remote_commit = git(Some(&source), &["rev-parse", "--verify", &remote_ref])?;
            let local_commit = git(Some(&source), &["rev-parse", "HEAD"])?;
            if local_commit != remote_commit {
                execute(
                    &app,
                    &jobs,
                    "build",
                    "Updating source",
                    "git",
                    &["merge".into(), "--ff-only".into(), remote_ref],
                    Some(&source),
                    None,
                )?;
            }
            execute(
                &app,
                &jobs,
                "build",
                "Updating submodules",
                "git",
                &[
                    "submodule".into(),
                    "update".into(),
                    "--init".into(),
                    "--recursive".into(),
                ],
                Some(&source),
                None,
            )?;
            let commit = git(Some(&source), &["rev-parse", "HEAD"])?;
            if commit != remote_commit {
                return Err("The checkout is ahead of the tracked remote; refusing to replace local commits".into());
            }
            let conn = db(&app)?;
            let existing: Option<String> = conn
                .query_row(
                    "SELECT path FROM builds WHERE repo_id=? AND commit_sha=? AND build_type=? ORDER BY created_at DESC LIMIT 1",
                    params![id, commit, options.build_type],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(path) = existing {
                if verify_emulator(&Path::new(&path).join(emulator_binary_name())).is_ok() {
                    mark_repository_build_commit(&app, id, &commit, &branch)?;
                    if activate_after_build {
                        let existing_id: i64 = conn
                            .query_row("SELECT id FROM builds WHERE path=?", [&path], |row| {
                                row.get(0)
                            })
                            .map_err(|error| error.to_string())?;
                        activate_build(app.clone(), existing_id)?;
                    }
                    return Ok(format!(
                        "Commit {} is already installed and verified; no rebuild was needed{}",
                        &commit[..12],
                        if activate_after_build {
                            " and it is now active"
                        } else {
                            ""
                        }
                    ));
                }
            }
            let path = build_and_register(
                &app, &jobs, id, &source, &commit, &options, &compiler, "build", true,
            )?;
            mark_repository_build_commit(&app, id, &commit, &branch)?;
            if activate_after_build {
                let connection = db(&app)?;
                let build_id: i64 = connection
                    .query_row(
                        "SELECT id FROM builds WHERE path=?",
                        [path.display().to_string()],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                activate_build(app.clone(), build_id)?;
                Ok(format!(
                    "Update build verified and activated at {}",
                    path.display()
                ))
            } else {
                Ok(format!(
                    "Update build verified at {}. The active build was left unchanged.",
                    path.display()
                ))
            }
        })();
        finish_job(&app, &jobs, "build", result);
    });
    Ok(job_id)
}
fn request_job_cancellation(jobs: &Jobs) -> Result<(), String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if !jobs.active.load(Ordering::SeqCst) || !jobs.cancellable.load(Ordering::SeqCst) {
        return Err("No cancellable job is running".into());
    }
    jobs.cancel_requested.store(true, Ordering::SeqCst);
    let pid = jobs.pid.load(Ordering::SeqCst);
    if pid == 0 {
        return Ok(());
    }
    stop_process_group(pid)
}
#[tauri::command]
fn cancel_job(jobs: tauri::State<'_, Arc<Jobs>>) -> Result<(), String> {
    request_job_cancellation(jobs.inner())
}
#[tauri::command]
fn validate_build(app: tauri::AppHandle, id: i64) -> Result<String, String> {
    let c = db(&app)?;
    let b = build(&c, id)?;
    let validation = verify_emulator(&Path::new(&b.path).join(emulator_binary_name()));
    match validation {
        Ok(()) => {
            c.execute(
                "UPDATE builds SET validation_status='verified' WHERE id=?",
                [id],
            )
            .map_err(|error| error.to_string())?;
            Ok("Emulator executable and --game CLI verified".into())
        }
        Err(error) => {
            c.execute(
                "UPDATE builds SET validation_status=? WHERE id=?",
                params![format!("failed: {error}"), id],
            )
            .map_err(|db_error| format!("{error}; could not save validation result: {db_error}"))?;
            Err(error)
        }
    }
}
#[tauri::command]
fn activate_build(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let mut c = db(&app)?;
    let b = build(&c, id)?;
    verify_emulator(&Path::new(&b.path).join(emulator_binary_name()))?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE builds SET validation_status='verified' WHERE id=?",
        [id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("UPDATE builds SET active=0", [])
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE builds SET active=1 WHERE id=?", [id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
#[tauri::command]
fn remove_build(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let _lifecycle = SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|_| "Session lifecycle is unavailable")?;
    let active: bool = db(&app)?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE build_id=? AND ended_at IS NULL)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if active {
        return Err("Stop the active game session before removing its build".into());
    }
    save_data::backup_build_saves_locked(&app, id)?;
    trophies::backup_build_trophies(&app, id)?;
    let mut c = db(&app)?;
    let root = state_paths(&app)?.data.join("builds");
    remove_managed_build(&mut c, id, &root)
}
fn retire_missing_build(c: &mut Connection, id: i64) -> Result<(), String> {
    let (active, path): (i64, String) = c
        .query_row(
            "SELECT active,path FROM builds WHERE id=? AND removed_at IS NULL",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Emulator build was not found".to_string())?;
    if Path::new(&path).is_dir() {
        return Err("Build installation still exists".into());
    }
    let running_session: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE build_id=? AND ended_at IS NULL)",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if running_session {
        return Ok(());
    }
    let replacement: Option<i64> = if active != 0 {
        let mut statement = c.prepare(
            "SELECT id,path FROM builds WHERE id<>? AND removed_at IS NULL AND validation_status='verified' ORDER BY created_at DESC,id DESC",
        ).map_err(|error| error.to_string())?;
        let candidates = statement
            .query_map([id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        candidates
            .into_iter()
            .find(|(_, candidate)| emulator_executable(Path::new(candidate)).is_file())
            .map(|(candidate, _)| candidate)
    } else {
        None
    };
    let tx = c.transaction().map_err(|error| error.to_string())?;
    if active != 0 {
        tx.execute("UPDATE builds SET active=0 WHERE active=1", [])
            .map_err(|error| error.to_string())?;
        if let Some(replacement) = replacement {
            tx.execute(
                "UPDATE builds SET active=1 WHERE id=? AND removed_at IS NULL",
                [replacement],
            )
            .map_err(|error| error.to_string())?;
        }
    }
    tx.execute("UPDATE games SET build_id=NULL WHERE build_id=?", [id])
        .map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE builds SET active=0,removed_at=? WHERE id=? AND removed_at IS NULL",
        params![now(), id],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}
fn remove_managed_build(c: &mut Connection, id: i64, managed_root: &Path) -> Result<(), String> {
    let (path, active): (String, i64) = c
        .query_row("SELECT path,active FROM builds WHERE id=?", [id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|_| "Emulator build was not found".to_string())?;
    let replacement: Option<i64> = if active != 0 {
        let mut statement = c
            .prepare(
                "SELECT id,path FROM builds WHERE id<>? AND removed_at IS NULL AND validation_status='verified' ORDER BY created_at DESC,id DESC",
            )
            .map_err(|e| e.to_string())?;
        let candidates = statement
            .query_map([id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        candidates
            .into_iter()
            .find(|(_, path)| emulator_executable(Path::new(path)).is_file())
            .map(|(candidate, _)| candidate)
    } else {
        None
    };
    let running_session: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE build_id=? AND ended_at IS NULL)",
            [id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if running_session {
        return Err("This build has a running emulator session".into());
    }
    if !Path::new(&path).exists() {
        return retire_missing_build(c, id);
    }
    let root = fs::canonicalize(managed_root)
        .map_err(|_| "Managed emulator build directory is unavailable".to_string())?;
    let original = PathBuf::from(path);
    let metadata = fs::symlink_metadata(&original)
        .map_err(|e| format!("Build installation is unavailable: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Build path is not a managed installation directory".into());
    }
    let canonical = fs::canonicalize(&original).map_err(|e| e.to_string())?;
    if paths_equal(&canonical, &root) || !path_starts_with(&canonical, &root) {
        return Err("Build path is outside Studio's managed installations".into());
    }
    let name = canonical
        .file_name()
        .ok_or("Invalid installation directory")?
        .to_string_lossy();
    let tombstone = canonical.with_file_name(format!(".{name}.removing-{id}-{}", now()));
    fs::rename(&canonical, &tombstone)
        .map_err(|e| format!("Could not safely stage build removal: {e}"))?;
    let result = (|| {
        let tx = c.transaction().map_err(|e| e.to_string())?;
        if let Some(replacement) = replacement {
            tx.execute("UPDATE builds SET active=0 WHERE active=1", [])
                .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE builds SET active=1 WHERE id=? AND removed_at IS NULL",
                [replacement],
            )
            .map_err(|e| e.to_string())?;
        } else if active != 0 {
            tx.execute("UPDATE builds SET active=0 WHERE id=?", [id])
                .map_err(|e| e.to_string())?;
        }
        tx.execute("UPDATE games SET build_id=NULL WHERE build_id=?", [id])
            .map_err(|e| e.to_string())?;
        let changed = tx
            .execute(
                "UPDATE builds SET active=0,removed_at=? WHERE id=? AND removed_at IS NULL",
                params![now(), id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("Build changed while removal was being prepared".to_string());
        }
        tx.commit().map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        let _ = fs::rename(&tombstone, &canonical);
        return Err(error);
    }
    fs::remove_dir_all(&tombstone)
        .map_err(|e| format!("Build entry removed, but staged files could not be cleaned up: {e}"))
}
fn emulator_help(executable: &Path) -> Result<String, String> {
    if !is_emulator_executable(executable) {
        return Err("Emulator path is not an executable file".into());
    }
    let mut command = Command::new(executable);
    command
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    prepare_owned_child(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not run emulator validation: {e}"))?;
    attach_owned_child(&mut child)?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Emulator --help stdout unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Emulator --help stderr unavailable")?;
    let stdout_reader =
        thread::spawn(move || read_capped_output(BufReader::new(stdout), EMULATOR_HELP_MAX_BYTES));
    let stderr_reader =
        thread::spawn(move || read_capped_output(BufReader::new(stderr), EMULATOR_HELP_MAX_BYTES));
    let wait_result =
        wait_for_child_with_timeout(&mut child, Duration::from_secs(5), "Emulator --help");
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Emulator --help stdout reader failed")?
        .map_err(|error| format!("Could not read emulator help: {error}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Emulator --help stderr reader failed")?
        .map_err(|error| format!("Could not read emulator help errors: {error}"))?;
    let status = wait_result?;
    if stdout.1 || stderr.1 {
        return Err(format!(
            "Emulator --help output exceeded the {} MiB safety limit",
            EMULATOR_HELP_MAX_BYTES / (1024 * 1024)
        ));
    }
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr.0).trim().to_string();
        return Err(if detail.is_empty() {
            format!("Emulator --help exited with {status}")
        } else {
            format!("Emulator --help exited with {status}: {detail}")
        });
    }
    String::from_utf8(stdout.0).map_err(|_| "Emulator --help output is not valid UTF-8".to_string())
}

fn verify_emulator(executable: &Path) -> Result<(), String> {
    let help = emulator_help(executable)?;
    if !help.contains("--game") {
        return Err("Emulator help did not advertise the supported --game argument".into());
    }
    Ok(())
}
#[tauri::command]
fn add_game(app: tauri::AppHandle, path: String) -> Result<Game, String> {
    let (game, _) = add_game_with_size(app.clone(), path, false)?;
    spawn_game_size_task(
        app,
        NEXT_GAME_SCAN_ID.fetch_add(1, Ordering::Relaxed),
        vec![(game.id, game.path.clone())],
        false,
    );
    Ok(game)
}
#[tauri::command]
fn refresh_game_metadata(app: tauri::AppHandle, game_id: i64) -> Result<(), String> {
    let c = db(&app)?;
    let registered = game(&c, game_id)?;
    let path = fs::canonicalize(&registered.path)
        .map_err(|_| "The registered game path is missing or inaccessible".to_string())?;
    if !supported_game_path(&path) {
        return Err("The registered game no longer has a supported game layout".into());
    }
    let (title, title_id, game_version, icon_path) = game_metadata(&path);
    c.execute(
        "UPDATE games SET title=CASE WHEN ?<>'' THEN ? ELSE title END,title_id=COALESCE(?,title_id),game_version=COALESCE(?,game_version),icon_path=COALESCE(?,icon_path),install_size=NULL WHERE id=?",
        params![title, title, title_id, game_version, icon_path, game_id],
    )
    .map_err(|error| error.to_string())?;
    spawn_game_size_task(
        app.clone(),
        NEXT_GAME_SCAN_ID.fetch_add(1, Ordering::Relaxed),
        vec![(game_id, path.display().to_string())],
        true,
    );
    Ok(())
}
fn add_game_with_size(
    app: tauri::AppHandle,
    path: String,
    calculate_install_size: bool,
) -> Result<(Game, bool), String> {
    let p = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if !supported_game_path(&p) {
        return Err("Choose a game directory with eboot.bin, an ELF, or a .zar archive".into());
    }
    let (mut title, title_id, game_version, icon_path) = game_metadata(&p);
    if title.is_empty() {
        title = p
            .file_stem()
            .or_else(|| p.file_name())
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
    }
    let install_size = if calculate_install_size {
        calculate_game_size(&p)
    } else {
        None
    };
    let c = db(&app)?;
    let inserted = c.execute(
        "INSERT OR IGNORE INTO games(path,title,title_id,game_version,icon_path,install_size) VALUES(?,?,?,?,?,?)",
        params![p.display().to_string(), title, title_id, game_version, icon_path, install_size],
    )
    .map_err(|e| e.to_string())?;
    c.execute(
        "UPDATE games SET hidden=0,title=CASE WHEN title_id IS NULL AND ? IS NOT NULL THEN ? ELSE title END, title_id=COALESCE(title_id,?), game_version=COALESCE(game_version,?), icon_path=COALESCE(icon_path,?), install_size=COALESCE(install_size,?) WHERE path=?",
        params![title_id, title, title_id, game_version, icon_path, install_size, p.display().to_string()],
    ).map_err(|e| e.to_string())?;
    let id = c
        .query_row(
            "SELECT id FROM games WHERE path=?",
            [p.display().to_string()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok((game(&c, id)?, inserted != 0))
}
fn calculate_game_size(path: &Path) -> Option<i64> {
    // The emulator needs app0 as its launch path, but the installed title may
    // contain sibling data/sce_sys folders beside app0. Count the package root
    // for the displayed install size while leaving the registered launch path
    // untouched.
    let package_root = if path.is_dir()
        && path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("app0"))
        && path
            .parent()
            .is_some_and(|parent| parent.join("sce_sys").is_dir())
    {
        path.parent().unwrap_or(path)
    } else if path.file_name().is_some_and(is_eboot_filename)
        && path.parent().is_some_and(|parent| {
            parent
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("app0"))
        })
        && path
            .parent()
            .and_then(Path::parent)
            .is_some_and(|parent| parent.join("sce_sys").is_dir())
    {
        path.parent().and_then(Path::parent).unwrap_or(path)
    } else if path.is_dir() {
        path
    } else if path.file_name().is_some_and(is_eboot_filename)
        || path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("elf"))
    {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    if package_root.is_dir() {
        Some(
            walkdir::WalkDir::new(package_root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter_map(|entry| entry.metadata().ok())
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                .fold(0_i64, i64::saturating_add),
        )
    } else {
        fs::metadata(path)
            .ok()
            .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
    }
}
fn emit_game_scan(app: &tauri::AppHandle, event: GameScanEvent) {
    let _ = app.emit("studio-game-scan", event);
}
fn spawn_game_size_task(
    app: tauri::AppHandle,
    scan_id: u64,
    games: Vec<(i64, String)>,
    report_progress: bool,
) {
    thread::spawn(move || {
        let total = games.len() as u64;
        let mut completed = 0_u64;
        for (game_id, path) in games {
            let size = calculate_game_size(Path::new(&path));
            if let (Some(size), Ok(c)) = (size, db(&app)) {
                let _ = c.execute(
                    "UPDATE games SET install_size=? WHERE id=? AND (? OR install_size IS NULL)",
                    params![size, game_id, !report_progress],
                );
            }
            completed += 1;
            if report_progress {
                emit_game_scan(
                    &app,
                    GameScanEvent {
                        scan_id,
                        phase: "Calculating install sizes".into(),
                        completed,
                        total,
                        current: path,
                        done: completed == total,
                    },
                );
            }
        }
        if report_progress && total == 0 {
            emit_game_scan(
                &app,
                GameScanEvent {
                    scan_id,
                    phase: "Complete".into(),
                    completed: 0,
                    total: 0,
                    current: String::new(),
                    done: true,
                },
            );
        }
    });
}
#[tauri::command]
fn remove_game_entry(app: tauri::AppHandle, game_id: i64) -> Result<(), String> {
    let mut c = db(&app)?;
    soft_remove_game_entry(&mut c, game_id)
}
fn soft_remove_game_entry(c: &mut Connection, game_id: i64) -> Result<(), String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    tx.query_row(
        "SELECT id FROM games WHERE id=? AND hidden=0",
        [game_id],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|_| "Game is not in the library".to_string())?;
    let active: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE game_id=? AND ended_at IS NULL)",
            [game_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if active {
        return Err("Stop the active game session before removing its library entry".into());
    }
    tx.execute("UPDATE games SET hidden=1 WHERE id=?", [game_id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
fn create_game_session(
    c: &mut Connection,
    game_id: i64,
    build_id: i64,
    started: i64,
    log_path: &str,
) -> Result<i64, String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let visible: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id=? AND hidden=0)",
            [game_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !visible {
        return Err("This game entry is hidden or no longer registered".into());
    }
    tx.execute(
        "INSERT INTO sessions(game_id,build_id,started_at,log_path) VALUES(?,?,?,?)",
        params![game_id, build_id, started, log_path],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}
fn is_eboot_filename(name: &std::ffi::OsStr) -> bool {
    is_eboot_filename_for(name, cfg!(windows))
}

fn is_eboot_filename_for(name: &std::ffi::OsStr, windows: bool) -> bool {
    if windows {
        name.eq_ignore_ascii_case("eboot.bin")
    } else {
        name == "eboot.bin"
    }
}

fn supported_game_path(path: &Path) -> bool {
    if path.is_dir() {
        is_elf_file(&path.join("eboot.bin"))
    } else {
        if path.file_name().is_some_and(is_eboot_filename) {
            is_elf_file(path)
        } else {
            match path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("elf") => is_elf_file(path),
                Some("zar") => is_zarchive_file(path),
                _ => false,
            }
        }
    }
}
fn is_zarchive_file(path: &Path) -> bool {
    const FOOTER_SIZE: u64 = 144;
    const FOOTER_MAGIC: u32 = 0x169f_52d6;
    const FOOTER_VERSION: u32 = 0x61bf_3a01;
    const OFFSET_RECORD_SIZE: u64 = 40;
    const FILE_TREE_ENTRY_SIZE: u64 = 16;

    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() <= FOOTER_SIZE {
        return false;
    }
    let file_size = metadata.len();
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    if file.seek(SeekFrom::End(-(FOOTER_SIZE as i64))).is_err() {
        return false;
    }
    let mut footer = [0_u8; FOOTER_SIZE as usize];
    if file.read_exact(&mut footer).is_err() {
        return false;
    }
    let read_u32 = |offset: usize| {
        u32::from_be_bytes(
            footer[offset..offset + 4]
                .try_into()
                .expect("fixed footer slice"),
        )
    };
    let read_u64 = |offset: usize| {
        u64::from_be_bytes(
            footer[offset..offset + 8]
                .try_into()
                .expect("fixed footer slice"),
        )
    };
    if read_u64(128) != file_size
        || read_u32(136) != FOOTER_VERSION
        || read_u32(140) != FOOTER_MAGIC
    {
        return false;
    }
    let mut sections = [(0_u64, 0_u64); 6];
    for (index, section) in sections.iter_mut().enumerate() {
        let offset = index * 16;
        *section = (read_u64(offset), read_u64(offset + 8));
        if section
            .0
            .checked_add(section.1)
            .is_none_or(|end| end > file_size)
        {
            return false;
        }
    }
    let offset_records_size = sections[1].1;
    let names_size = sections[2].1;
    let file_tree_size = sections[3].1;
    if offset_records_size == 0
        || offset_records_size > u32::MAX as u64
        || offset_records_size % OFFSET_RECORD_SIZE != 0
        || names_size > 0x7fff_ffff
        || names_size == 0
        || file_tree_size == 0
        || file_tree_size > u32::MAX as u64
        || file_tree_size % FILE_TREE_ENTRY_SIZE != 0
    {
        return false;
    }
    if file.seek(SeekFrom::Start(sections[3].0)).is_err() {
        return false;
    }
    let mut root = [0_u8; FILE_TREE_ENTRY_SIZE as usize];
    if file.read_exact(&mut root).is_err() {
        return false;
    }
    let root_name_type = u32::from_be_bytes(root[0..4].try_into().unwrap());
    let root_start = u32::from_be_bytes(root[4..8].try_into().unwrap()) as u64;
    let root_count = u32::from_be_bytes(root[8..12].try_into().unwrap()) as u64;
    let entries = file_tree_size / FILE_TREE_ENTRY_SIZE;
    if root_name_type & 0x8000_0000 != 0
        || root_name_type & 0x7fff_ffff != 0x7fff_ffff
        || root_count > 100_000
        || root_start
            .checked_add(root_count)
            .is_none_or(|end| end > entries)
    {
        return false;
    }
    for child_index in root_start..root_start + root_count {
        let Some(child_offset) = sections[3]
            .0
            .checked_add(child_index.saturating_mul(FILE_TREE_ENTRY_SIZE))
        else {
            return false;
        };
        if file.seek(SeekFrom::Start(child_offset)).is_err() {
            return false;
        }
        let mut child = [0_u8; FILE_TREE_ENTRY_SIZE as usize];
        if file.read_exact(&mut child).is_err() {
            return false;
        }
        let name_and_type = u32::from_be_bytes(child[0..4].try_into().unwrap());
        if name_and_type & 0x8000_0000 == 0 {
            continue;
        }
        let name_offset = (name_and_type & 0x7fff_ffff) as u64;
        if name_offset >= names_size {
            return false;
        }
        let Some(name_position) = sections[2].0.checked_add(name_offset) else {
            return false;
        };
        if file.seek(SeekFrom::Start(name_position)).is_err() {
            return false;
        }
        let mut length_header = [0_u8; 2];
        if file.read_exact(&mut length_header[..1]).is_err() {
            return false;
        }
        let name_length = if length_header[0] & 0x80 == 0 {
            length_header[0] as u64
        } else {
            if name_offset + 2 > names_size || file.read_exact(&mut length_header[1..]).is_err() {
                return false;
            }
            ((length_header[0] & 0x7f) as u64) | ((length_header[1] as u64) << 7)
        };
        let name_start = name_position + if length_header[0] & 0x80 == 0 { 1 } else { 2 };
        if name_length > names_size
            || name_offset + (name_start - name_position) + name_length > names_size
        {
            return false;
        }
        if name_length != 9 {
            continue;
        }
        let mut name = [0_u8; 9];
        if file.seek(SeekFrom::Start(name_start)).is_err() || file.read_exact(&mut name).is_err() {
            return false;
        }
        if name.eq_ignore_ascii_case(b"eboot.bin") {
            return true;
        }
    }
    false
}
fn is_elf_file(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    if file.read_exact(&mut magic).is_err() {
        return false;
    }
    if magic == *b"\x7fELF" {
        return true;
    }
    let known_self_magic = magic == [0x4f, 0x15, 0x3d, 0x1d] || magic == [0x54, 0x14, 0xf5, 0xee];
    if !known_self_magic {
        return false;
    }
    let mut rest = [0u8; 24];
    if file.read_exact(&mut rest).is_err() {
        return false;
    }
    let ident_tail = &rest[..8];
    let unknown = u16::from_le_bytes([rest[22], rest[23]]);
    (ident_tail == [0x00, 0x01, 0x01, 0x12, 0x01, 0x01, 0x00, 0x00] && unknown == 0x22)
        || (ident_tail == [0x10, 0x01, 0x01, 0x12, 0x01, 0x01, 0x00, 0x10] && unknown == 0x32)
}
fn game_metadata(path: &Path) -> (String, Option<String>, Option<String>, Option<String>) {
    let root = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let system_directory = root.join("sce_sys");
    let meta = system_directory.join("param.json");
    let icon = system_directory.join("icon0.png");
    let empty = (String::new(), None, None, None);
    let Ok(bytes) = fs::read(&meta) else {
        return empty;
    };
    if bytes.len() > 1024 * 1024 {
        return empty;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return empty;
    };
    let title = value
        .get("localizedParameters")
        .and_then(|v| v.as_object())
        .and_then(|localized| {
            let default = localized.get("defaultLanguage").and_then(|v| v.as_str());
            default
                .and_then(|language| localized.get(language))
                .or_else(|| localized.values().next())
        })
        .and_then(|v| v.get("titleName"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let title_id = value
        .get("titleId")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let version = value
        .get("appVersion")
        .or_else(|| value.get("contentVersion"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let icon_path = icon.is_file().then(|| icon.to_string_lossy().into_owned());
    (title, title_id, version, icon_path)
}
#[tauri::command]
async fn read_game_icon(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<tauri::ipc::Response, String> {
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let c = db(&app)?;
        let game = game(&c, game_id)?;
        let Some(icon) = game.icon_path else {
            return Ok(Vec::new());
        };
        let Some(root) = game_artwork_root(Path::new(&game.path)) else {
            return Ok(Vec::new());
        };
        Ok(read_game_thumbnail(
            Path::new(&icon),
            &root,
            16 * 1024 * 1024,
            1024,
            1024,
            8 * 1024 * 1024,
        )
        .unwrap_or_default())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}
#[tauri::command]
async fn read_game_logo(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<tauri::ipc::Response, String> {
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let connection = db(&app)?;
        let game = game(&connection, game_id)?;
        let Some(root) = game_artwork_root(Path::new(&game.path)) else {
            return Ok(Vec::new());
        };
        let paths = state_paths(&app)?;
        let cache_dir = paths.cache.join("game-artwork");
        fs::create_dir_all(&cache_dir)
            .map_err(|error| format!("Could not create game artwork cache: {error}"))?;
        let cache_root = fs::canonicalize(&cache_dir)
            .map_err(|error| format!("Could not access game artwork cache: {error}"))?;
        let managed_cache = fs::canonicalize(&paths.cache)
            .map_err(|error| format!("Could not access app cache: {error}"))?;
        if !path_starts_with(&cache_root, &managed_cache)
            || fs::symlink_metadata(&cache_dir)
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("Game artwork cache is outside the managed app cache".into());
        }
        Ok(load_cached_game_logo(&root, &cache_root, game_id).unwrap_or_default())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}
#[tauri::command]
async fn read_game_background(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<tauri::ipc::Response, String> {
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let c = db(&app)?;
        let game = game(&c, game_id)?;
        let Some(root) = game_artwork_root(Path::new(&game.path)) else {
            return Ok(Vec::new());
        };
        Ok(read_game_thumbnail(
            &root.join("sce_sys/pic0.png"),
            &root,
            32 * 1024 * 1024,
            1600,
            900,
            8 * 1024 * 1024,
        )
        .unwrap_or_default())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}
fn game_artwork_root(game_path: &Path) -> Option<PathBuf> {
    if game_path.is_dir() {
        Some(game_path.to_path_buf())
    } else if game_path.file_name().is_some_and(is_eboot_filename)
        || game_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("elf"))
    {
        Some(game_path.parent().unwrap_or(game_path).to_path_buf())
    } else {
        None
    }
}
fn read_game_png(path: &Path, game_root: &Path, maximum_size: u64) -> Option<Vec<u8>> {
    let root = fs::canonicalize(game_root).ok()?;
    let path = fs::canonicalize(path).ok()?;
    if !path_starts_with(&path, &root) || !path.is_file() {
        return None;
    }
    let file = fs::File::open(&path).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() > maximum_size {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(maximum_size) as usize);
    file.take(maximum_size.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > maximum_size {
        return None;
    }
    bytes.starts_with(b"\x89PNG\r\n\x1a\n").then_some(bytes)
}
const MAX_ARTWORK_DECODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_ARTWORK_PIXELS: usize = MAX_ARTWORK_DECODED_BYTES / 4;
fn read_game_thumbnail(
    path: &Path,
    game_root: &Path,
    maximum_source_size: u64,
    maximum_width: u32,
    maximum_height: u32,
    maximum_output_size: usize,
) -> Option<Vec<u8>> {
    let bytes = read_game_png(path, game_root, maximum_source_size)?;
    let thumbnail = resize_png(&bytes, maximum_width, maximum_height)?;
    (thumbnail.len() <= maximum_output_size).then_some(thumbnail)
}
const MAX_LOGO_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_LOGO_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
fn read_game_artwork_source(path: &Path, game_root: &Path) -> Option<Vec<u8>> {
    let root = fs::canonicalize(game_root).ok()?;
    let path = fs::canonicalize(path).ok()?;
    if !path_starts_with(&path, &root) || !path.is_file() {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_LOGO_SOURCE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_LOGO_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_LOGO_SOURCE_BYTES).then_some(bytes)
}
fn transparent_logo_alpha(alpha_values: impl Iterator<Item = u8>, pixel_count: usize) -> bool {
    let minimum = (pixel_count / 1000).max(1);
    let (transparent, visible) = alpha_values.fold((0, 0), |(transparent, visible), alpha| {
        (
            transparent + usize::from(alpha <= 16),
            visible + usize::from(alpha >= 64),
        )
    });
    transparent >= minimum && visible >= minimum
}
fn png_has_transparent_logo(bytes: &[u8]) -> bool {
    let limits = png::Limits {
        bytes: MAX_ARTWORK_DECODED_BYTES,
    };
    let mut decoder = png::Decoder::new_with_limits(BufReader::new(Cursor::new(bytes)), limits);
    let (width, height) = match decoder.read_header_info() {
        Ok(header) => (header.width, header.height),
        Err(_) => return false,
    };
    let Some(pixel_count) = (width as usize).checked_mul(height as usize) else {
        return false;
    };
    if width == 0 || height == 0 || pixel_count > MAX_ARTWORK_PIXELS {
        return false;
    }
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let Ok(mut reader) = decoder.read_info() else {
        return false;
    };
    let Some(decoded_size) = reader.output_buffer_size() else {
        return false;
    };
    if decoded_size > MAX_ARTWORK_DECODED_BYTES {
        return false;
    }
    let mut decoded = vec![0; decoded_size];
    let Ok(frame) = reader.next_frame(&mut decoded) else {
        return false;
    };
    let pixels = &decoded[..frame.buffer_size()];
    match frame.color_type {
        png::ColorType::Rgba => {
            transparent_logo_alpha(pixels.iter().skip(3).step_by(4).copied(), pixel_count)
        }
        png::ColorType::GrayscaleAlpha => {
            transparent_logo_alpha(pixels.iter().skip(1).step_by(2).copied(), pixel_count)
        }
        _ => false,
    }
}
fn encode_rgba_png(width: u32, height: u32, pixels: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(pixels).ok()?;
    drop(writer);
    Some(bytes)
}
fn decode_dds_logo(bytes: &[u8]) -> Option<Vec<u8>> {
    let dds = image_dds::ddsfile::Dds::read(Cursor::new(bytes)).ok()?;
    let (width, height) = (dds.get_width(), dds.get_height());
    let pixel_count = (width as usize).checked_mul(height as usize)?;
    if width == 0
        || height == 0
        || pixel_count > MAX_ARTWORK_PIXELS
        || dds.get_depth() != 1
        || dds.get_num_array_layers() != 1
    {
        return None;
    }
    let surface = image_dds::SurfaceRgba8::decode_layers_mipmaps_dds(&dds, 0..1, 0..1).ok()?;
    let image = surface.get_image(0, 0, 0)?;
    if !transparent_logo_alpha(
        image.as_raw().iter().skip(3).step_by(4).copied(),
        pixel_count,
    ) {
        return None;
    }
    let png = encode_rgba_png(width, height, image.as_raw())?;
    let thumbnail = resize_png(&png, 1200, 500)?;
    (thumbnail.len() <= MAX_LOGO_OUTPUT_BYTES).then_some(thumbnail)
}
fn cache_game_logo(cache_root: &Path, game_id: i64, source: &[u8], logo: &[u8]) {
    let digest = format!("{:x}", Sha256::digest(source));
    let path = cache_root.join(format!("game-{game_id}-{digest}.png"));
    let temporary = cache_root.join(format!(".game-{game_id}-{}.tmp", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(logo)?;
        file.sync_all()?;
        replace_file(&temporary, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
}
fn load_cached_game_logo(game_root: &Path, cache_root: &Path, game_id: i64) -> Option<Vec<u8>> {
    let system = game_root.join("sce_sys");
    for (name, is_dds) in [("pic2.dds", true), ("pic2.png", false)] {
        let path = system.join(name);
        let Some(source) = read_game_artwork_source(&path, game_root) else {
            continue;
        };
        let digest = format!("{:x}", Sha256::digest(&source));
        let cached_path = cache_root.join(format!("game-{game_id}-{digest}.png"));
        if let Some(cached) = read_game_png(&cached_path, cache_root, MAX_LOGO_OUTPUT_BYTES as u64)
        {
            if png_has_transparent_logo(&cached) {
                return Some(cached);
            }
        }
        let logo = if is_dds {
            decode_dds_logo(&source)
        } else if png_has_transparent_logo(&source) {
            resize_png(&source, 1200, 500)
                .filter(|thumbnail| thumbnail.len() <= MAX_LOGO_OUTPUT_BYTES)
        } else {
            None
        };
        let Some(logo) = logo else {
            continue;
        };
        cache_game_logo(cache_root, game_id, &source, &logo);
        return Some(logo);
    }
    None
}
fn resize_png(bytes: &[u8], maximum_width: u32, maximum_height: u32) -> Option<Vec<u8>> {
    if maximum_width == 0 || maximum_height == 0 {
        return None;
    }
    let limits = png::Limits {
        bytes: MAX_ARTWORK_DECODED_BYTES,
    };
    let mut decoder = png::Decoder::new_with_limits(BufReader::new(Cursor::new(bytes)), limits);
    let (source_width, source_height) = {
        let info = decoder.read_header_info().ok()?;
        (info.width, info.height)
    };
    let pixel_count = (source_width as usize).checked_mul(source_height as usize)?;
    if source_width == 0 || source_height == 0 || pixel_count > MAX_ARTWORK_PIXELS {
        return None;
    }
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    let decoded_size = reader.output_buffer_size()?;
    if decoded_size > MAX_ARTWORK_DECODED_BYTES {
        return None;
    }
    let mut decoded = vec![0; decoded_size];
    let frame = reader.next_frame(&mut decoded).ok()?;
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return None,
    };
    if frame.bit_depth != png::BitDepth::Eight
        || frame.width != source_width
        || frame.height != source_height
    {
        return None;
    }
    let source = &decoded[..frame.buffer_size()];
    let scale = (maximum_width as f64 / source_width as f64)
        .min(maximum_height as f64 / source_height as f64)
        .min(1.0);
    let output_width = ((source_width as f64 * scale).floor() as u32).max(1);
    let output_height = ((source_height as f64 * scale).floor() as u32).max(1);
    let output_pixels = (output_width as usize).checked_mul(output_height as usize)?;
    let mut resized = vec![0_u8; output_pixels.checked_mul(4)?];
    let scale_x = source_width as f64 / output_width as f64;
    let scale_y = source_height as f64 / output_height as f64;
    for y in 0..output_height {
        let source_y =
            ((y as f64 + 0.5) * scale_y - 0.5).clamp(0.0, source_height.saturating_sub(1) as f64);
        let y0 = source_y.floor() as u32;
        let y1 = (y0 + 1).min(source_height - 1);
        let fy = source_y - y0 as f64;
        for x in 0..output_width {
            let source_x = ((x as f64 + 0.5) * scale_x - 0.5)
                .clamp(0.0, source_width.saturating_sub(1) as f64);
            let x0 = source_x.floor() as u32;
            let x1 = (x0 + 1).min(source_width - 1);
            let fx = source_x - x0 as f64;
            let output_index = ((y * output_width + x) * 4) as usize;
            for channel in 0..4 {
                let sample = |sample_x: u32, sample_y: u32| {
                    let index = ((sample_y * source_width + sample_x) as usize) * channels;
                    (match frame.color_type {
                        png::ColorType::Rgba => source[index + channel],
                        png::ColorType::Rgb => {
                            if channel == 3 {
                                255
                            } else {
                                source[index + channel]
                            }
                        }
                        png::ColorType::GrayscaleAlpha => {
                            if channel == 3 {
                                source[index + 1]
                            } else {
                                source[index]
                            }
                        }
                        png::ColorType::Grayscale => {
                            if channel == 3 {
                                255
                            } else {
                                source[index]
                            }
                        }
                        png::ColorType::Indexed => 0,
                    }) as f64
                };
                let top = sample(x0, y0) * (1.0 - fx) + sample(x1, y0) * fx;
                let bottom = sample(x0, y1) * (1.0 - fx) + sample(x1, y1) * fx;
                resized[output_index + channel] = (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
        }
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, output_width, output_height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&resized).ok()?;
    }
    Some(output)
}
fn directory_contains_game_eboot(directory: &Path, windows: bool) -> bool {
    let conventional = directory.join("eboot.bin");
    if conventional.is_file() && is_elf_file(&conventional) {
        return true;
    }
    if !windows {
        return false;
    }
    // Windows installations can preserve the upstream EBOOT.BIN spelling,
    // including in case-sensitive directories. Find that entry once per
    // directory instead of reopening it for every file below the directory.
    fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| is_eboot_filename_for(&entry.file_name(), true) && is_elf_file(&entry.path()))
}

fn has_game_ancestor_cached(
    path: &Path,
    scan_root: &Path,
    eboot_cache: &mut HashMap<PathBuf, bool>,
    windows: bool,
) -> bool {
    let Some(mut directory) = path.parent() else {
        return false;
    };
    loop {
        if !path_starts_with(directory, scan_root) {
            return false;
        }
        let contains_eboot = *eboot_cache
            .entry(directory.to_path_buf())
            .or_insert_with(|| directory_contains_game_eboot(directory, windows));
        if contains_eboot {
            return true;
        }
        if directory == scan_root {
            return false;
        }
        let Some(parent) = directory.parent() else {
            return false;
        };
        directory = parent;
    }
}
fn scan_entry_is_symlink(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_symlink()
}
#[tauri::command]
async fn scan_games(app: tauri::AppHandle, path: String) -> Result<GameScanResult, String> {
    tauri::async_runtime::spawn_blocking(move || scan_games_sync(app, path))
        .await
        .map_err(|error| format!("Game scan task failed: {error}"))?
}

fn scan_games_sync(app: tauri::AppHandle, path: String) -> Result<GameScanResult, String> {
    let root = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("Select a directory".into());
    }
    let scan_id = NEXT_GAME_SCAN_ID.fetch_add(1, Ordering::Relaxed);
    emit_game_scan(
        &app,
        GameScanEvent {
            scan_id,
            phase: "Scanning folder".into(),
            completed: 0,
            total: 0,
            current: root.display().to_string(),
            done: false,
        },
    );
    let mut imported = 0_usize;
    let mut already_registered = 0_usize;
    let mut rejected = 0_usize;
    let mut candidates = HashSet::new();
    let mut eboot_cache = HashMap::new();
    let mut entries_seen = 0_u64;
    let mut games_for_size = Vec::new();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                rejected += 1;
                continue;
            }
        };
        entries_seen += 1;
        let p = entry.path();
        if scan_entry_is_symlink(&entry) {
            rejected += 1;
            continue;
        }
        let is_eboot = is_eboot_filename(entry.file_name()) && p.is_file();
        if !is_eboot && has_game_ancestor_cached(p, &root, &mut eboot_cache, cfg!(windows)) {
            continue;
        }
        let archive_extension = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("zar"))
            .unwrap_or(false);
        let elf_extension = p
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("elf"))
            .unwrap_or(false);
        if is_eboot && !is_elf_file(p)
            || archive_extension && !is_zarchive_file(p)
            || elf_extension && !is_elf_file(p)
        {
            rejected += 1;
            continue;
        }
        if is_eboot || archive_extension || elf_extension {
            let candidate = if is_eboot {
                p.parent().unwrap_or(&root)
            } else {
                p
            };
            let Ok(candidate) = fs::canonicalize(candidate) else {
                rejected += 1;
                continue;
            };
            if !candidates.insert(candidate.clone()) {
                continue;
            }
            match add_game_with_size(app.clone(), candidate.display().to_string(), false) {
                Ok((game, was_inserted)) => {
                    if was_inserted {
                        imported += 1;
                    } else {
                        already_registered += 1;
                    }
                    if game.install_size.is_none() {
                        games_for_size.push((game.id, game.path));
                    }
                }
                Err(_) => rejected += 1,
            }
        }
        if entries_seen.is_multiple_of(64) {
            emit_game_scan(
                &app,
                GameScanEvent {
                    scan_id,
                    phase: "Scanning folder".into(),
                    completed: entries_seen,
                    total: 0,
                    current: p.display().to_string(),
                    done: false,
                },
            );
        }
    }
    emit_game_scan(
        &app,
        GameScanEvent {
            scan_id,
            phase: "Registering games".into(),
            completed: imported.saturating_add(already_registered) as u64,
            total: imported.saturating_add(already_registered) as u64,
            current: if rejected == 0 {
                String::new()
            } else {
                format!("{rejected} unsupported or inaccessible entries skipped")
            },
            done: false,
        },
    );
    spawn_game_size_task(app.clone(), scan_id, games_for_size, true);
    Ok(GameScanResult {
        imported,
        already_registered,
        rejected,
    })
}
#[tauri::command]
fn set_game_build(
    app: tauri::AppHandle,
    game_id: i64,
    build_id: Option<i64>,
) -> Result<(), String> {
    let c = db(&app)?;
    if let Some(id) = build_id {
        build(&c, id)?;
    }
    c.execute(
        "UPDATE games SET build_id=? WHERE id=?",
        params![build_id, game_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn toggle_favorite(app: tauri::AppHandle, game_id: i64) -> Result<(), String> {
    let c = db(&app)?;
    c.execute("UPDATE games SET favorite=1-favorite WHERE id=?", [game_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn get_profile(app: tauri::AppHandle, game_id: i64) -> Result<Profile, String> {
    let c = db(&app)?;
    game(&c, game_id)?;
    profile(&c, game_id)
}
#[tauri::command]
fn inspect_game_content(app: tauri::AppHandle, game_id: i64) -> Result<serde_json::Value, String> {
    let c = db(&app)?;
    let selected = game(&c, game_id)?;
    let selected_build = build_for_game(&c, game_id).ok();
    let game_options = profile(&c, game_id)?.options;
    let global_options = selected_build
        .as_ref()
        .map(|build| build_profile_options(&c, build.id))
        .transpose()?
        .unwrap_or_default();
    let user_id = game_options
        .get("--user-id")
        .or_else(|| global_options.get("--user-id"))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1000);
    let selected_path = fs::canonicalize(&selected.path).map_err(|e| e.to_string())?;
    let root = if selected_path.is_dir() {
        selected_path
    } else {
        selected_path
            .parent()
            .ok_or("Game path has no containing directory")?
            .to_path_buf()
    };
    let mut trophies = Vec::new();
    for trophy_dir in [root.join("sce_sys/trophy2"), root.join("sce_sys/trophy")] {
        if let Ok(entries) = fs::read_dir(&trophy_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_none_or(|x| !x.eq_ignore_ascii_case("ucp"))
                {
                    continue;
                }
                let bytes = match fs::read(&path) {
                    Ok(b) if b.len() >= 64 && b.len() <= 256 * 1024 * 1024 => b,
                    _ => continue,
                };
                let be32 = |o: usize| -> Option<u32> {
                    Some(u32::from_be_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
                };
                let be64 = |o: usize| -> Option<u64> {
                    Some(u64::from_be_bytes(bytes.get(o..o + 8)?.try_into().ok()?))
                };
                if be32(0) != Some(0xb228c60a) || be32(4) != Some(1) {
                    continue;
                }
                let (Some(count), Some(toc)) = (be32(16), be32(20)) else {
                    continue;
                };
                if count > 4096 {
                    continue;
                }
                let mut conf = None;
                let mut metadata_by_language = BTreeMap::new();
                for i in 0..count as usize {
                    let Some(base) = (toc as usize)
                        .checked_add(32)
                        .and_then(|x| x.checked_add(i * 64))
                    else {
                        break;
                    };
                    let Some(name_bytes) = bytes.get(base..base + 32) else {
                        break;
                    };
                    let name = String::from_utf8_lossy(
                        &name_bytes[..name_bytes.iter().position(|b| *b == 0).unwrap_or(32)],
                    )
                    .to_ascii_lowercase();
                    let (Some(off), Some(size)) = (be64(base + 32), be64(base + 40)) else {
                        continue;
                    };
                    let (Ok(off), Ok(size)) = (usize::try_from(off), usize::try_from(size)) else {
                        continue;
                    };
                    let Some(data) = off.checked_add(size).and_then(|end| bytes.get(off..end))
                    else {
                        continue;
                    };
                    if name == "tropconf.json" {
                        conf = serde_json::from_slice::<serde_json::Value>(data).ok();
                    } else if name.starts_with("tropmeta_") && name.ends_with(".json") {
                        let language = name
                            .strip_prefix("tropmeta_")
                            .and_then(|name| name.strip_suffix(".json"))
                            .unwrap_or("unknown")
                            .to_owned();
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) {
                            metadata_by_language.insert(language, value);
                        }
                    }
                }
                let Some(conf) = conf else {
                    continue;
                };
                let default_language = conf
                    .get("defaultLanguage")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let metadata = metadata_by_language
                    .get("en-us")
                    .or_else(|| metadata_by_language.get("en-gb"))
                    .or_else(|| metadata_by_language.get("en"))
                    .or_else(|| metadata_by_language.get(&default_language))
                    .or_else(|| metadata_by_language.values().next());
                let trophy_id = |value: &serde_json::Value| {
                    let text = value
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| value.as_number().map(ToString::to_string))?;
                    Some(
                        text.parse::<u32>()
                            .map(|id| format!("{id:04}"))
                            .unwrap_or(text),
                    )
                };
                let text_map: HashMap<String, serde_json::Value> = metadata
                    .and_then(|value| value.get("metadata"))
                    .and_then(|m| m.get("trophyMetadata"))
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|item| {
                        let id = trophy_id(item.get("id")?)?;
                        Some((id, item.clone()))
                    })
                    .collect();
                for def in conf
                    .get("trophies")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    let id = def.get("id").and_then(trophy_id).unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    let text = text_map.get(&id);
                    trophies.push(serde_json::json!({"id":id,"grade":def.get("grade"),"hidden":def.get("hidden").and_then(|v|v.as_bool()).unwrap_or(false),"name":text.and_then(|v|v.get("name")).and_then(|v|v.as_str()).unwrap_or(""),"detail":text.and_then(|v|v.get("detail")).and_then(|v|v.as_str()).unwrap_or(""),"unlocked":null}));
                }
            }
        }
    }
    let mut unlock_data_available = false;
    if let (Some(title_id), Some(build)) = (selected.title_id.as_deref(), selected_build.as_ref()) {
        let unlock_dir = Path::new(&build.path).join("_Trophies").join(title_id);
        if let Ok(entries) = fs::read_dir(unlock_dir) {
            let mut state_files = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if !filename.starts_with(&format!("trophies_{user_id}_"))
                    || path.extension().is_none_or(|ext| ext != "json")
                {
                    continue;
                }
                state_files.push(path);
            }
            // GameContent has one unlocked flag per definition. Only select it when
            // there is exactly one local service file for the resolved emulator user;
            // combining service labels would invent progress provenance.
            if state_files.len() == 1 {
                let mut unlocked = HashSet::new();
                if let Ok(bytes) = fs::read(&state_files[0]) {
                    if bytes.len() <= 1024 * 1024 {
                        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if let Some(ids) = state
                                .get("unlockedTrophies")
                                .and_then(|value| value.as_array())
                            {
                                for id in ids.iter().filter_map(|value| {
                                    value.as_u64().or_else(|| value.as_str()?.parse().ok())
                                }) {
                                    unlocked.insert(id);
                                }
                                unlock_data_available = true;
                            }
                        }
                    }
                }
                if unlock_data_available {
                    for trophy in &mut trophies {
                        let id = trophy
                            .get("id")
                            .and_then(|value| value.as_str())
                            .and_then(|value| value.parse::<u64>().ok());
                        if let Some(id) = id {
                            trophy["unlocked"] = serde_json::json!(unlocked.contains(&id));
                        }
                    }
                }
            }
        }
    }
    let options = game_options;
    let installed_patch = selected_build
        .as_ref()
        .and_then(|build| {
            selected
                .title_id
                .as_deref()
                .map(|id| build_patch_path(build, id))
        })
        .filter(|path| path.is_file());
    let configured_patch = options
        .get("--game-patch")
        .or_else(|| global_options.get("--game-patch"))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let patch_path = installed_patch.as_ref().or(configured_patch.as_ref());
    let patch_source = patch_path.map(|path| path.to_string_lossy().into_owned());
    let mut cheats = Vec::new();
    let mut cheat_error = None;
    if let Some(path) = patch_path {
        match fs::metadata(path)
            .ok()
            .filter(|m| m.is_file() && m.len() <= 8 * 1024 * 1024)
            .and_then(|_| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
        {
            Some(doc) => {
                let mods = doc.get("mods").and_then(|v| v.as_array());
                if mods.is_none() {
                    cheat_error = Some("The configured patch is valid JSON but does not contain a GoldHEN-style mods list.");
                }
                for m in mods.into_iter().flatten() {
                    cheats.push(serde_json::json!({"name":m.get("name").and_then(|v|v.as_str()).unwrap_or("Unnamed mod"),"enabled":m.get("enabled").and_then(|v|v.as_bool()).unwrap_or(true),"title_id":doc.get("id"),"version":doc.get("version")}));
                }
            }
            None => {
                cheat_error =
                    Some("The configured game patch could not be parsed as GoldHEN-style JSON.")
            }
        }
    }
    Ok(
        serde_json::json!({"trophies":trophies,"trophy_unlock_state":if unlock_data_available {"available"} else {"unavailable"},"patches":cheats,"patch_source":patch_source,"patch_error":cheat_error}),
    )
}
const PS5_CHEAT_INDEX: &str =
    "https://raw.githubusercontent.com/TeeKay87/HEN-Cheats-Collection/master/cheats/json.txt";
const PS5_CHEAT_FILE_BASE: &str =
    "https://raw.githubusercontent.com/TeeKay87/HEN-Cheats-Collection/master/cheats/json/";
fn read_remote_text(url: &str, max_bytes: usize) -> Result<String, String> {
    let mut response = ureq::get(url)
        .header("User-Agent", "KytyPS5-Studio")
        .call()
        .map_err(|error| format!("Cheat repository request failed: {error}"))?;
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| format!("Could not read cheat repository response: {error}"))?;
    if body.len() > max_bytes {
        return Err("Cheat repository response exceeded the size limit".into());
    }
    String::from_utf8(body)
        .map_err(|error| format!("Cheat repository returned invalid UTF-8: {error}"))
}
fn valid_cheat_filename(filename: &str) -> bool {
    filename.to_ascii_lowercase().ends_with(".json")
        && filename.len() <= 180
        && filename
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}
fn normalize_cheat_title_id(value: &str) -> Option<String> {
    let id = value.trim().split('_').next().unwrap_or_default();
    if id.len() < 8 || id.len() > 16 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    Some(id.to_ascii_uppercase())
}
fn remote_cheat_entry(
    filename: &str,
    title: &str,
    title_id: &str,
) -> Result<serde_json::Value, String> {
    let source = read_remote_text(&format!("{PS5_CHEAT_FILE_BASE}{filename}"), 8 * 1024 * 1024)?;
    let document: serde_json::Value = serde_json::from_str(&source)
        .map_err(|error| format!("Cheat file {filename} is invalid JSON: {error}"))?;
    if document
        .get("id")
        .and_then(|value| value.as_str())
        .and_then(normalize_cheat_title_id)
        .as_deref()
        != normalize_cheat_title_id(title_id).as_deref()
    {
        return Err(format!(
            "Cheat file {filename} does not match game ID {title_id}"
        ));
    }
    let mods = document
        .get("mods")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("Cheat file {filename} has no supported mods list"))?;
    let version = document
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let items = mods
        .iter()
        .filter_map(|item| {
            Some(serde_json::json!({
                "name": item.get("name")?.as_str()?,
                "type": item.get("type").and_then(|value| value.as_str()).unwrap_or("unknown")
            }))
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "filename": filename,
        "title": document.get("name").and_then(|value| value.as_str()).unwrap_or(title),
        "version": version,
        "mods": items,
        "source": format!("https://github.com/TeeKay87/HEN-Cheats-Collection/blob/master/cheats/json/{filename}")
    }))
}
#[tauri::command]
fn fetch_game_cheats(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<Vec<serde_json::Value>, String> {
    let connection = db(&app)?;
    let selected = game(&connection, game_id)?;
    let Some(title_id) = selected
        .title_id
        .as_deref()
        .and_then(normalize_cheat_title_id)
    else {
        return Ok(Vec::new());
    };
    let index = read_remote_text(PS5_CHEAT_INDEX, 4 * 1024 * 1024)?;
    let mut filenames = index
        .lines()
        .filter_map(|line| line.trim().split_once('='))
        .filter(|(filename, _)| {
            valid_cheat_filename(filename.trim())
                && filename
                    .trim()
                    .split('_')
                    .next()
                    .and_then(normalize_cheat_title_id)
                    .as_deref()
                    == Some(title_id.as_str())
        })
        .map(|(filename, title)| (filename.trim().to_owned(), title.trim().to_owned()))
        .take(100)
        .collect::<Vec<_>>();
    filenames.sort_by_key(|(filename, _)| {
        let version = filename.split('_').nth(1).unwrap_or("");
        (
            version != selected.game_version.as_deref().unwrap_or(""),
            version.to_owned(),
        )
    });
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    for (filename, title) in &filenames {
        match remote_cheat_entry(filename, title, &title_id) {
            Ok(entry) => entries.push(entry),
            Err(error) => failures.push(error),
        }
    }
    if entries.is_empty() && !failures.is_empty() {
        return Err(format!(
            "Found {} matching cheat file(s), but none could be read: {}",
            failures.len(),
            failures[0]
        ));
    }
    Ok(entries)
}
#[tauri::command]
fn download_game_cheat(
    app: tauri::AppHandle,
    game_id: i64,
    filename: String,
) -> Result<String, String> {
    if !valid_cheat_filename(&filename) {
        return Err("Cheat filename is not valid".into());
    }
    let connection = db(&app)?;
    let selected = game(&connection, game_id)?;
    let title_id = selected
        .title_id
        .as_deref()
        .and_then(normalize_cheat_title_id)
        .ok_or("Game has no valid title ID")?;
    if filename
        .split('_')
        .next()
        .and_then(normalize_cheat_title_id)
        .as_deref()
        != Some(title_id.as_str())
    {
        return Err("Cheat file does not belong to this game".into());
    }
    let source = read_remote_text(&format!("{PS5_CHEAT_FILE_BASE}{filename}"), 8 * 1024 * 1024)?;
    validate_game_patch_document(&selected, source.as_bytes())?;
    let selected_build = build_for_game(&connection, game_id)?;
    let directory = Path::new(&selected_build.path).join("_Patches");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let destination = directory.join(format!("{title_id}.json"));
    write_patch_file(&destination, source.as_bytes())?;
    Ok(destination.display().to_string())
}

fn build_patch_path(build: &Build, title_id: &str) -> PathBuf {
    Path::new(&build.path)
        .join("_Patches")
        .join(format!("{}.json", title_id.trim().to_ascii_uppercase()))
}

fn validate_game_patch_document(
    selected: &Game,
    content: &[u8],
) -> Result<serde_json::Value, String> {
    let expected_id = selected
        .title_id
        .as_deref()
        .and_then(normalize_cheat_title_id)
        .ok_or("This game needs a recognized title ID before a patch can be added")?;
    let document: serde_json::Value = serde_json::from_slice(content)
        .map_err(|error| format!("Patch is not valid JSON: {error}"))?;
    if document
        .get("id")
        .and_then(|value| value.as_str())
        .and_then(normalize_cheat_title_id)
        .as_deref()
        != Some(expected_id.as_str())
    {
        return Err(format!(
            "Patch title ID must match this game ({expected_id})"
        ));
    }
    let version = document
        .get("version")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or("Patch must include its exact game version")?;
    if let Some(game_version) = selected.game_version.as_deref() {
        if version.trim() != game_version.trim() {
            return Err(format!(
                "Patch is for game version {version}, but this game is version {game_version}. Import the matching patch file."
            ));
        }
    }
    if document
        .get("process")
        .and_then(|value| value.as_str())
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err("Patch must include the game process name (normally eboot.bin)".into());
    }
    let mods = document
        .get("mods")
        .and_then(|value| value.as_array())
        .filter(|mods| !mods.is_empty())
        .ok_or("Patch must contain a non-empty GoldHEN-style mods array")?;
    if mods.iter().any(|item| {
        !item.is_object()
            || item.get("name").and_then(|value| value.as_str()).is_none()
            || item.get("enabled").is_some_and(|value| !value.is_boolean())
            || item
                .get("memory")
                .and_then(|value| value.as_array())
                .is_none()
    }) {
        return Err(
            "Each patch entry must have a name, a memory array and an optional boolean enabled field"
                .into(),
        );
    }
    Ok(document)
}

fn write_patch_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Patch path has no parent folder".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create _Patches folder: {error}"))?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temporary, content)
        .map_err(|error| format!("Could not write patch file: {error}"))?;
    if let Err(error) = replace_file(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Could not install patch file: {error}"));
    }
    Ok(())
}

fn set_patch_mod_enabled(
    document: &mut serde_json::Value,
    mod_index: usize,
    enabled: bool,
) -> Result<(), String> {
    let mods = document
        .get_mut("mods")
        .and_then(|value| value.as_array_mut())
        .ok_or("Installed patch has no mods list")?;
    let patch_mod = mods
        .get_mut(mod_index)
        .ok_or("Patch list changed. Refresh the Patches tab and try again")?;
    patch_mod["enabled"] = serde_json::Value::Bool(enabled);
    Ok(())
}

fn store_custom_game_patch(
    app: &tauri::AppHandle,
    game_id: i64,
    content: &[u8],
) -> Result<String, String> {
    const MAX_CUSTOM_PATCH_BYTES: usize = 8 * 1024 * 1024;
    if content.is_empty() || content.len() > MAX_CUSTOM_PATCH_BYTES {
        return Err("Patch file must be between 1 byte and 8 MiB".into());
    }
    let conn = db(app)?;
    let selected = game(&conn, game_id)?;
    let document = validate_game_patch_document(&selected, content)?;
    let selected_build = build_for_game(&conn, game_id)?;
    let destination = build_patch_path(
        &selected_build,
        selected.title_id.as_deref().unwrap_or_default(),
    );
    let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    write_patch_file(&destination, &bytes)?;
    Ok(destination.display().to_string())
}

#[tauri::command]
fn set_game_patch_mod_enabled(
    app: tauri::AppHandle,
    game_id: i64,
    mod_index: usize,
    enabled: bool,
) -> Result<String, String> {
    let conn = db(&app)?;
    let selected = game(&conn, game_id)?;
    let selected_build = build_for_game(&conn, game_id)?;
    let path = build_patch_path(
        &selected_build,
        selected.title_id.as_deref().unwrap_or_default(),
    );
    let bytes =
        fs::read(&path).map_err(|error| format!("Could not read installed patch: {error}"))?;
    let mut document = validate_game_patch_document(&selected, &bytes)?;
    set_patch_mod_enabled(&mut document, mod_index, enabled)?;
    let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    write_patch_file(&path, &bytes)?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

#[tauri::command]
fn import_game_patch(
    app: tauri::AppHandle,
    game_id: i64,
    path: String,
) -> Result<serde_json::Value, String> {
    let source =
        fs::canonicalize(path).map_err(|error| format!("Could not open patch file: {error}"))?;
    if source
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("json"))
    {
        return Err("Only GoldHEN-style JSON patch files can be imported".into());
    }
    let metadata = fs::metadata(&source).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err("Patch file must be a regular file no larger than 8 MiB".into());
    }
    let content = fs::read(source).map_err(|error| error.to_string())?;
    let saved_path = store_custom_game_patch(&app, game_id, &content)?;
    let content = String::from_utf8(content)
        .map_err(|_| "Patch JSON must be encoded as UTF-8".to_string())?;
    Ok(serde_json::json!({"path": saved_path, "content": content}))
}

#[tauri::command]
fn save_custom_game_patch(
    app: tauri::AppHandle,
    game_id: i64,
    content: String,
) -> Result<String, String> {
    store_custom_game_patch(&app, game_id, content.as_bytes())
}

#[tauri::command]
fn get_profile_capabilities(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<ProfileCapabilities, String> {
    let c = db(&app)?;
    let selected = build_for_game(&c, game_id)?;
    capabilities_for_build(&selected, &repo(&c, selected.repo_id)?)
}
#[tauri::command]
fn get_global_profile_options(app: tauri::AppHandle) -> Result<BTreeMap<String, String>, String> {
    global_profile_options(&db(&app)?)
}
#[tauri::command]
fn get_active_profile_capabilities(app: tauri::AppHandle) -> Result<ProfileCapabilities, String> {
    let c = db(&app)?;
    let id: i64 = c
        .query_row("SELECT id FROM builds WHERE active=1 LIMIT 1", [], |r| {
            r.get(0)
        })
        .map_err(|_| "No active emulator build".to_string())?;
    let selected = build(&c, id)?;
    capabilities_for_build(&selected, &repo(&c, selected.repo_id)?)
}
#[tauri::command]
fn get_build_profile_capabilities(
    app: tauri::AppHandle,
    build_id: i64,
) -> Result<ProfileCapabilities, String> {
    let c = db(&app)?;
    let selected = build(&c, build_id)?;
    capabilities_for_build(&selected, &repo(&c, selected.repo_id)?)
}
#[tauri::command]
fn get_build_profile_options(
    app: tauri::AppHandle,
    build_id: i64,
) -> Result<BTreeMap<String, String>, String> {
    let c = db(&app)?;
    build(&c, build_id)?;
    build_profile_options(&c, build_id)
}
#[tauri::command]
fn set_global_profile_options(
    app: tauri::AppHandle,
    mut options: BTreeMap<String, String>,
) -> Result<(), String> {
    let c = db(&app)?;
    let build_id: i64 = c
        .query_row("SELECT id FROM builds WHERE active=1 LIMIT 1", [], |r| {
            r.get(0)
        })
        .map_err(|_| {
            "Select and activate an emulator build before configuring global defaults".to_string()
        })?;
    let selected = build(&c, build_id)?;
    let capabilities = capabilities_for_build(&selected, &repo(&c, selected.repo_id)?)?;
    validate_extra_profile_options(&options, &capabilities)?;
    for (flag, value) in &mut options {
        if [
            "--game-patch",
            "--printf-output-file",
            "--addcont-dir",
            "--shader-log-folder",
            "--command-buffer-dump-folder",
        ]
        .contains(&flag.as_str())
        {
            *value = canonicalize_option_path(flag, value)?;
        }
    }
    let json = serde_json::to_string(&options).map_err(|e| e.to_string())?;
    c.execute("INSERT INTO global_profile_defaults(id,options_json) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET options_json=excluded.options_json", [json]).map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn set_build_profile_options(
    app: tauri::AppHandle,
    build_id: i64,
    mut options: BTreeMap<String, String>,
) -> Result<(), String> {
    let c = db(&app)?;
    let selected_build = build(&c, build_id)?;
    let capabilities = capabilities_for_build(&selected_build, &repo(&c, selected_build.repo_id)?)?;
    validate_extra_profile_options(&options, &capabilities)?;
    for (flag, value) in &mut options {
        if [
            "--game-patch",
            "--printf-output-file",
            "--addcont-dir",
            "--shader-log-folder",
            "--command-buffer-dump-folder",
        ]
        .contains(&flag.as_str())
        {
            *value = canonicalize_option_path(flag, value)?;
        }
    }
    let json = serde_json::to_string(&options).map_err(|error| error.to_string())?;
    c.execute(
        "INSERT INTO build_profile_defaults(build_id,options_json) VALUES(?,?) ON CONFLICT(build_id) DO UPDATE SET options_json=excluded.options_json",
        params![build_id, json],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
fn validate_profile_values(profile: &Profile) -> Result<(), String> {
    for value in [profile.screen_width, profile.screen_height]
        .into_iter()
        .flatten()
    {
        if value == 0 {
            return Err("Resolution must be greater than zero".into());
        }
    }
    if let Some(mode) = &profile.present_mode {
        if !["Fifo", "Mailbox", "Immediate"].contains(&mode.as_str()) {
            return Err("Unsupported present mode".into());
        }
    }
    if profile.controller_color.as_ref().is_some_and(|color| {
        let bytes = color.as_bytes();
        bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
    }) {
        return Err("Controller color must use #RRGGBB format".into());
    }
    for value in [profile.controller_volume, profile.controller_vibration]
        .into_iter()
        .flatten()
    {
        if value > 100 {
            return Err("Controller volume and vibration must be between 0 and 100".into());
        }
    }
    if profile.keyboard_player.is_some_and(|player| player > 4) {
        return Err("Keyboard player must be between 0 and 4".into());
    }
    Ok(())
}
#[tauri::command]
fn set_profile(app: tauri::AppHandle, game_id: i64, mut profile: Profile) -> Result<(), String> {
    merge_legacy_profile_options(&mut profile);
    sync_legacy_profile_fields(&mut profile);
    validate_profile_values(&profile)?;
    let c = db(&app)?;
    game(&c, game_id)?;
    let selected_build = build_for_game(&c, game_id)?;
    let capabilities = capabilities_for_build(&selected_build, &repo(&c, selected_build.repo_id)?)?;
    validate_extra_profile_options(&profile.options, &capabilities)?;
    for (flag, value) in &mut profile.options {
        if [
            "--game-patch",
            "--printf-output-file",
            "--addcont-dir",
            "--shader-log-folder",
            "--command-buffer-dump-folder",
        ]
        .contains(&flag.as_str())
        {
            *value = canonicalize_option_path(flag, value)?;
        }
    }
    if profile.screen_width.is_some()
        && !capabilities
            .verified_options
            .iter()
            .any(|f| f == "--screen-width")
        || profile.screen_height.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--screen-height")
        || profile.present_mode.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--present-mode")
        || profile.perf_overlay
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--perf-overlay")
        || profile.fullscreen
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--fullscreen")
        || profile.controller_color.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--controller-color")
        || profile.controller_volume.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--controller-volume")
        || profile.controller_vibration.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--controller-vibration")
        || profile.keyboard_player.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--keyboard-player")
    {
        return Err(
            "This emulator build does not support one or more selected profile options".into(),
        );
    }
    let options_json = serde_json::to_string(&profile.options).map_err(|e| e.to_string())?;
    c.execute("INSERT INTO profiles(game_id,screen_width,screen_height,present_mode,perf_overlay,fullscreen,controller_color,controller_volume,controller_vibration,keyboard_player,options_json) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(game_id) DO UPDATE SET screen_width=excluded.screen_width,screen_height=excluded.screen_height,present_mode=excluded.present_mode,perf_overlay=excluded.perf_overlay,fullscreen=excluded.fullscreen,controller_color=excluded.controller_color,controller_volume=excluded.controller_volume,controller_vibration=excluded.controller_vibration,keyboard_player=excluded.keyboard_player,options_json=excluded.options_json",params![game_id,profile.screen_width,profile.screen_height,profile.present_mode,profile.perf_overlay,profile.fullscreen,profile.controller_color,profile.controller_volume,profile.controller_vibration,profile.keyboard_player,options_json]).map_err(|e|e.to_string())?;
    Ok(())
}
#[cfg(unix)]
fn steam_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(windows)]
fn steam_registry_path() -> Option<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_READ, REG_SZ,
    };

    fn read_value(root: HKEY, subkey: &str, value_name: &str) -> Option<PathBuf> {
        let subkey = subkey.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let value_name = value_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut key = std::ptr::null_mut();
        if unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &mut key) } != 0 {
            return None;
        }
        let path = (|| {
            let mut value_type = 0;
            let mut byte_count = 0;
            if unsafe {
                RegQueryValueExW(
                    key,
                    value_name.as_ptr(),
                    std::ptr::null(),
                    &mut value_type,
                    std::ptr::null_mut(),
                    &mut byte_count,
                )
            } != 0
                || value_type != REG_SZ
                || byte_count < 2
                || byte_count % 2 != 0
                || byte_count > 65_536
            {
                return None;
            }
            let mut data = vec![0_u16; (byte_count / 2) as usize];
            if unsafe {
                RegQueryValueExW(
                    key,
                    value_name.as_ptr(),
                    std::ptr::null(),
                    &mut value_type,
                    data.as_mut_ptr().cast(),
                    &mut byte_count,
                )
            } != 0
                || value_type != REG_SZ
            {
                return None;
            }
            let end = data
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(data.len());
            let value = OsString::from_wide(&data[..end]);
            (!value.is_empty()).then(|| PathBuf::from(value))
        })();
        unsafe { RegCloseKey(key) };
        path
    }

    [
        (HKEY_CURRENT_USER, "Software\\Valve\\Steam"),
        (HKEY_LOCAL_MACHINE, "Software\\Valve\\Steam"),
        (HKEY_LOCAL_MACHINE, "Software\\WOW6432Node\\Valve\\Steam"),
    ]
    .into_iter()
    .find_map(|(root, key)| {
        read_value(root, key, "SteamPath").or_else(|| read_value(root, key, "InstallPath"))
    })
}

#[cfg(any(windows, test))]
fn windows_command_line_quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                quoted.push(character);
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(any(windows, test))]
fn steam_windows_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    let utf16_len = path.encode_utf16().count();
    if utf16_len >= 260 {
        return path.into_owned();
    }
    if let Some(unc_path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc_path}")
    } else if let Some(path) = path.strip_prefix(r"\\?\") {
        path.to_owned()
    } else {
        path.into_owned()
    }
}

use self::steam::add_game_to_steam as add_game_to_steam_impl;
#[cfg(test)]
use self::steam_shortcuts::*;

fn steam_config_dir() -> Result<PathBuf, String> {
    #[cfg(unix)]
    let h = home()?;
    #[cfg(unix)]
    let roots = [
        h.join(".steam/steam"),
        h.join(".steam/root"),
        h.join(".local/share/Steam"),
        h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    #[cfg(windows)]
    let roots = {
        let mut roots = Vec::new();
        if let Some(root) = env::var_os("STEAM_PATH").filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(root));
        }
        if let Some(root) = steam_registry_path() {
            roots.push(root);
        }
        for key in ["PROGRAMFILES(X86)", "PROGRAMFILES", "LOCALAPPDATA"] {
            if let Some(root) = env::var_os(key).filter(|value| !value.is_empty()) {
                let root = PathBuf::from(root);
                if key == "LOCALAPPDATA" {
                    roots.push(root.join("Programs").join("Steam"));
                } else {
                    roots.push(root.join("Steam"));
                }
            }
        }
        roots
    };
    let mut candidates = Vec::new();
    for root in roots {
        let userdata = root.join("userdata");
        let Ok(users) = fs::read_dir(userdata) else {
            continue;
        };
        for user in users.flatten() {
            if !user
                .file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
                || user.file_name() == "0"
            {
                continue;
            }
            let config = user.path().join("config");
            let shortcuts = config.join("shortcuts.vdf");
            let localconfig = config.join("localconfig.vdf");
            if !config.is_dir() {
                continue;
            }
            let timestamp = shortcuts
                .metadata()
                .or_else(|_| localconfig.metadata())
                .and_then(|m| m.modified())
                .ok();
            candidates.push((timestamp, shortcuts.exists(), config));
        }
    }
    candidates.sort_by_key(|(time, has_shortcuts, _)| (time.is_some(), *time, *has_shortcuts));
    candidates.pop().map(|(_, _, path)| path).ok_or_else(|| {
        "Steam user configuration was not found. Install and open Steam once, then retry.".into()
    })
}

#[tauri::command]
fn add_game_to_steam(app: tauri::AppHandle, game_id: i64, build_id: i64) -> Result<String, String> {
    add_game_to_steam_impl(app, game_id, build_id)
}

#[tauri::command]
fn launch_game(app: tauri::AppHandle, game_id: i64) -> Result<(), String> {
    launch_game_impl(app, game_id, None, None)
}
#[tauri::command]
fn launch_game_ab(
    app: tauri::AppHandle,
    game_id: i64,
    build_id: i64,
    settings_build_id: i64,
) -> Result<(), String> {
    launch_game_impl(app, game_id, Some(build_id), Some(settings_build_id))
}
fn launch_game_impl(
    app: tauri::AppHandle,
    game_id: i64,
    requested_build_id: Option<i64>,
    settings_build_id: Option<i64>,
) -> Result<(), String> {
    if SHADER_PRECOMPILE_ACTIVE.load(Ordering::Acquire) {
        return Err("Wait for shader precompilation to finish before launching a game".into());
    }
    let _session_lifecycle = SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    if SHADER_PRECOMPILE_ACTIVE.load(Ordering::Acquire) {
        return Err("Wait for shader precompilation to finish before launching a game".into());
    }
    if app.state::<Arc<Jobs>>().updating.load(Ordering::SeqCst) {
        return Err("KytyPS5 Studio is installing an application update".into());
    }
    let mut c = db(&app)?;
    let g = game(&c, game_id)?;
    let game_profile = profile(&c, game_id)?;
    let build_id = if let Some(id) = requested_build_id {
        id
    } else if let Some(id) = g.build_id {
        id
    } else {
        c.query_row("SELECT id FROM builds WHERE active=1 LIMIT 1", [], |r| {
            r.get(0)
        })
        .map_err(|_| "Select an active emulator build".to_string())?
    };
    let b = build(&c, build_id)?;
    let selected_repo = repo(&c, b.repo_id)?;
    let supports_background_precompile = source_proves_background_precompile(&b, &selected_repo);
    let enable_shader_precompile = supports_background_precompile
        && get_game_shader_precompile_enabled(&c, game_id, build_id)?;
    let capabilities = capabilities_for_build(&b, &selected_repo)?;
    let settings_build_id = settings_build_id.unwrap_or(build_id);
    let settings_defaults = build_profile_options(&c, settings_build_id)?;
    let mut effective_options = if settings_build_id == build_id {
        effective_profile_options(&settings_defaults, &game_profile.options, &capabilities)?
    } else {
        let settings_build = build(&c, settings_build_id)?;
        let settings_repo = repo(&c, settings_build.repo_id)?;
        let settings_capabilities = capabilities_for_build(&settings_build, &settings_repo)?;
        let shared = effective_profile_options(
            &settings_defaults,
            &game_profile.options,
            &settings_capabilities,
        )?;
        if let Some(unsupported) = shared.keys().find(|flag| {
            !capabilities
                .verified_options
                .iter()
                .any(|supported| supported == *flag)
        }) {
            return Err(format!(
                "A/B test stopped: the target build does not support {unsupported}, so both builds cannot use identical settings"
            ));
        }
        validate_extra_profile_options(&shared, &capabilities)?;
        shared
    };
    if effective_options
        .get("--gpu")
        .is_some_and(|value| value.parse::<i32>().is_ok_and(|index| index >= 0))
    {
        validate_gpu_hardware_selection(
            &effective_options,
            &graphics_diagnostics().vulkan_details,
        )?;
    }
    for (flag, value) in &mut effective_options {
        if [
            "--game-patch",
            "--printf-output-file",
            "--addcont-dir",
            "--shader-log-folder",
            "--command-buffer-dump-folder",
        ]
        .contains(&flag.as_str())
        {
            *value = canonicalize_option_path(flag, value)?;
        }
    }
    if let Some(title_id) = g.title_id.as_deref().and_then(normalize_cheat_title_id) {
        let installed_patch = build_patch_path(&b, &title_id);
        if installed_patch.is_file() {
            let metadata = fs::metadata(&installed_patch).map_err(|error| error.to_string())?;
            if metadata.len() > 8 * 1024 * 1024 {
                return Err("Installed game patch exceeds the 8 MiB safety limit".into());
            }
            let patch_bytes = fs::read(&installed_patch).map_err(|error| error.to_string())?;
            validate_game_patch_document(&g, &patch_bytes)?;
            if !capabilities
                .verified_options
                .iter()
                .any(|option| option == "--game-patch")
            {
                return Err("The selected emulator build does not support game patches".into());
            }
            effective_options.insert(
                "--game-patch".into(),
                fs::canonicalize(&installed_patch)
                    .map_err(|error| format!("Could not resolve installed game patch: {error}"))?
                    .display()
                    .to_string(),
            );
        }
    }
    if game_profile.screen_width.is_some()
        && !capabilities
            .verified_options
            .iter()
            .any(|f| f == "--screen-width")
        || game_profile.screen_height.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--screen-height")
        || game_profile.present_mode.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--present-mode")
        || game_profile.perf_overlay
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--perf-overlay")
        || game_profile.fullscreen
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--fullscreen")
        || game_profile.controller_color.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--controller-color")
        || game_profile.controller_volume.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--controller-volume")
        || game_profile.controller_vibration.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--controller-vibration")
        || game_profile.keyboard_player.is_some()
            && !capabilities
                .verified_options
                .iter()
                .any(|f| f == "--keyboard-player")
    {
        return Err("The selected emulator build no longer supports this game's saved profile. Clear unsupported profile options before launching.".into());
    }
    let exe = Path::new(&b.path).join(emulator_binary_name());
    let game_path = fs::canonicalize(&g.path)
        .map_err(|_| "The registered game path is missing or inaccessible".to_string())?;
    if !supported_game_path(&game_path) {
        return Err(
            "The registered game path no longer contains a supported eboot.bin, ELF, or ZArchive"
                .into(),
        );
    }
    if !is_emulator_executable(&exe) {
        return Err("Emulator or game path no longer exists".into());
    }
    let log = state_paths(&app)?.data.join("logs");
    fs::create_dir_all(&log).map_err(|e| e.to_string())?;
    let log_path = log.join(format!("game-{}-{}.log", game_id, now()));
    let logfile = Arc::new(Mutex::new(SessionLogWriter::new(
        fs::File::create(&log_path).map_err(|e| e.to_string())?,
        SESSION_LOG_MAX_BYTES,
    )));
    let started = now();
    let session_id = create_game_session(
        &mut c,
        game_id,
        build_id,
        started,
        &log_path.display().to_string(),
    )?;
    let mut command = Command::new(&exe);
    command.arg("--game").arg(&game_path);
    if supports_background_precompile {
        command.env(
            "KYTY_SHADER_PRECOMPILE",
            if enable_shader_precompile { "1" } else { "0" },
        );
        command.env(
            "KYTY_PIPELINE_FAST_FIRST",
            if enable_shader_precompile { "1" } else { "0" },
        );
    }
    command.args(extra_profile_arguments(
        &effective_options,
        &capabilities.switch_options,
    ));
    command
        .current_dir(&b.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    prepare_owned_child(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            c.execute(
                "UPDATE sessions SET ended_at=?,outcome='launch_error' WHERE id=?",
                params![now(), session_id],
            )
            .map_err(|db_error| {
                format!("Could not start emulator ({error}); session update failed: {db_error}")
            })?;
            emit(
                &app,
                "game",
                "Launch failed",
                error.to_string(),
                true,
                false,
            );
            return Err(format!("Could not start emulator: {error}"));
        }
    };
    if let Err(error) = attach_owned_child(&mut child) {
        let _ = c.execute(
            "UPDATE sessions SET ended_at=?,outcome='ownership_error' WHERE id=? AND ended_at IS NULL",
            params![now(), session_id],
        );
        return Err(format!(
            "Could not safely manage the emulator process: {error}"
        ));
    }
    let pid = child.id();
    let process = (0..10).find_map(|_| {
        let stat = proc_stat(pid);
        if stat.is_none() {
            thread::sleep(Duration::from_millis(10));
        }
        stat
    });
    let Some(process) = process else {
        let (cleanup, wait) = stop_and_reap_owned_child(&mut child);
        let message = "Could not verify the emulator process identity";
        let _ = c.execute(
            "UPDATE sessions SET ended_at=?,outcome='identity_error' WHERE id=? AND ended_at IS NULL",
            params![now(), session_id],
        );
        return Err(match (cleanup, wait) {
            (Ok(()), Ok(_)) => message.to_string(),
            (cleanup, wait) => format!(
                "{message}; process cleanup: {}; child wait: {}",
                cleanup.err().unwrap_or_else(|| "ok".into()),
                wait.err().unwrap_or_else(|| "ok".into())
            ),
        });
    };
    let tracking = c.execute(
        "UPDATE sessions SET process_pid=?,process_start_ticks=? WHERE id=?",
        params![pid as i64, process.start_ticks as i64, session_id],
    );
    if let Err(error) = tracking {
        let (cleanup, wait) = stop_and_reap_owned_child(&mut child);
        let _ = c.execute(
            "UPDATE sessions SET ended_at=?,outcome='tracking_error' WHERE id=?",
            params![now(), session_id],
        );
        return Err(format!(
            "Emulator started as PID {pid}, but session tracking failed: {error}{}{}",
            cleanup
                .err()
                .map(|error| format!("; process cleanup failed: {error}"))
                .unwrap_or_default(),
            wait.err()
                .map(|error| format!("; child wait failed: {error}"))
                .unwrap_or_default()
        ));
    }
    emit(&app, "game", "Running", format!("PID {pid}"), false, true);
    thread::spawn(move || {
        let result = (|| -> Result<(Option<i32>, String), String> {
            let setup = configure_session_output(&child);
            if let Err(error) = setup {
                let (cleanup, wait) = stop_and_reap_owned_child(&mut child);
                return Err(format!(
                    "Could not prepare emulator log pipes: {error}{}{}",
                    cleanup
                        .err()
                        .map(|error| format!("; process cleanup failed: {error}"))
                        .unwrap_or_default(),
                    wait.err()
                        .map(|error| format!("; child wait failed: {error}"))
                        .unwrap_or_default()
                ));
            }
            let stop_readers = Arc::new(AtomicBool::new(false));
            let stdout_reader = child.stdout.take().map(|stdout| {
                spawn_session_output_reader(
                    stdout,
                    "stdout",
                    logfile.clone(),
                    app.clone(),
                    stop_readers.clone(),
                )
            });
            let stderr_reader = child.stderr.take().map(|stderr| {
                spawn_session_output_reader(
                    stderr,
                    "stderr",
                    logfile.clone(),
                    app.clone(),
                    stop_readers.clone(),
                )
            });
            let wait_result = wait_for_child_and_cleanup_group(&mut child, || {});
            stop_readers.store(true, Ordering::SeqCst);
            let (status, cleanup) = wait_result?;
            if let Err(error) = &cleanup {
                let _ = logfile
                    .lock()
                    .unwrap()
                    .append_line("process cleanup error", error);
            }
            if let Some(reader) = stdout_reader {
                reader
                    .join()
                    .map_err(|_| "stdout reader thread panicked".to_string())?;
            }
            if let Some(reader) = stderr_reader {
                reader
                    .join()
                    .map_err(|_| "stderr reader thread panicked".to_string())?;
            }
            let outcome = if cleanup.is_err() {
                "cleanup_error"
            } else if status.success() {
                "normal"
            } else if status.code().is_some() {
                "error"
            } else {
                "crash"
            };
            Ok((status.code(), outcome.into()))
        })();
        if let Ok(mut c) = db(&app) {
            let (code, outcome) = result
                .as_ref()
                .map(|(code, outcome)| (*code, outcome.as_str()))
                .unwrap_or((None, "launch_error"));
            let ended = now();
            if let Ok(tx) = c.transaction() {
                let recorded = tx.execute(
                    "UPDATE sessions SET ended_at=?,exit_code=?,outcome=? WHERE id=? AND ended_at IS NULL",
                    params![ended, code, outcome, session_id],
                );
                if recorded.is_ok_and(|count| count == 1) {
                    let _ = tx.execute(
                        "UPDATE games SET play_seconds=play_seconds+?,last_played=? WHERE id=?",
                        params![ended.saturating_sub(started).max(0), started, game_id],
                    );
                }
                let _ = tx.commit();
            }
        }
        save_data::auto_backup_after_session(&app, game_id);
        trophies::auto_backup_after_session(&app, game_id);
        emit(
            &app,
            "game",
            "Complete",
            result
                .as_ref()
                .map(|(_, o)| o.clone())
                .unwrap_or_else(|e| e.clone()),
            true,
            result
                .as_ref()
                .is_ok_and(|(_, outcome)| outcome == "normal"),
        );
    });
    Ok(())
}
#[derive(Serialize)]
struct ProcessMetric {
    session_id: i64,
    game_id: i64,
    build_id: i64,
    title: String,
    commit_sha: String,
    build_type: String,
    pid: Option<u32>,
    runtime_seconds: i64,
    cpu_percent: Option<f64>,
    resident_bytes: Option<u64>,
    status: String,
}
#[tauri::command]
fn process_metrics(
    app: tauri::AppHandle,
    samples: tauri::State<'_, ProcessSamples>,
) -> Result<Vec<ProcessMetric>, String> {
    let conn = db(&app)?;
    let mut statement = conn
        .prepare("SELECT s.id,s.game_id,s.build_id,g.title,s.started_at,s.process_pid,s.process_start_ticks,b.commit_sha,b.build_type,b.path FROM sessions s JOIN games g ON g.id=s.game_id JOIN builds b ON b.id=s.build_id WHERE s.ended_at IS NULL ORDER BY s.started_at")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let active: Vec<_> = rows
        .map(|row| row.map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    drop(statement);

    #[cfg(unix)]
    let clock_ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    #[cfg(windows)]
    let clock_ticks = windows_process::cpu_clock_ticks_per_second();
    let now_instant = Instant::now();
    let now_wall = now();
    let mut previous = samples.0.lock().map_err(|e| e.to_string())?;
    let mut active_ids = HashSet::new();
    let mut lost: Vec<(i64, Option<i64>, Option<u32>, &'static str)> = Vec::new();
    let mut result = Vec::with_capacity(active.len());
    for (
        session_id,
        game_id,
        build_id,
        title,
        started_at,
        pid,
        expected_start,
        commit_sha,
        build_type,
        build_path,
    ) in active
    {
        active_ids.insert(session_id);
        let runtime = now_wall.saturating_sub(started_at).max(0);
        let Some(pid) = pid.and_then(|pid| u32::try_from(pid).ok()) else {
            let status = if runtime > 10 {
                lost.push((session_id, None, None, "launch_interrupted"));
                "Launch interrupted; process was not recorded"
            } else {
                "Starting"
            };
            result.push(ProcessMetric {
                session_id,
                game_id,
                build_id,
                title,
                commit_sha,
                build_type,
                pid: None,
                runtime_seconds: runtime,
                cpu_percent: None,
                resident_bytes: None,
                status: status.into(),
            });
            continue;
        };

        let expected_executable = Path::new(&build_path).join(emulator_binary_name());
        let stat = expected_start.and_then(|expected| {
            proc_stat(pid).filter(|stat| {
                verified_session_process(
                    i64::from(pid),
                    Some(expected),
                    *stat,
                    process_executable_matches(pid, &expected_executable),
                )
            })
        });
        let Some(stat) = stat else {
            let sample = previous.entry(session_id).or_default();
            sample.missing_samples = sample.missing_samples.saturating_add(1);
            let status = if sample.missing_samples >= PROCESS_MISSING_SAMPLE_LIMIT {
                lost.push((session_id, Some(started_at), Some(pid), "process_lost"));
                "Process ended; exit status unavailable"
            } else {
                "Checking process status"
            };
            result.push(ProcessMetric {
                session_id,
                game_id,
                build_id,
                title,
                commit_sha,
                build_type,
                pid: Some(pid),
                runtime_seconds: runtime,
                cpu_percent: None,
                resident_bytes: None,
                status: status.into(),
            });
            continue;
        };

        let sample = previous.entry(session_id).or_default();
        sample.missing_samples = 0;
        let cpu_percent = sample.previous.and_then(|(ticks, sampled_at)| {
            cpu_percent(
                ticks,
                stat.cpu_ticks,
                now_instant.duration_since(sampled_at),
                clock_ticks,
            )
        });
        sample.previous = Some((stat.cpu_ticks, now_instant));
        let status = match stat.state {
            'R' => "Running",
            'S' => "Sleeping",
            'D' => "Waiting for I/O",
            'T' | 't' => "Stopped",
            'Z' | 'X' => "Exiting",
            _ => "Running",
        };
        result.push(ProcessMetric {
            session_id,
            game_id,
            build_id,
            title,
            commit_sha,
            build_type,
            pid: Some(pid),
            runtime_seconds: runtime,
            cpu_percent,
            resident_bytes: Some(stat.resident_bytes),
            status: status.into(),
        });
    }
    previous.retain(|session_id, _| {
        active_ids.contains(session_id)
            && !lost.iter().any(|(lost_id, _, _, _)| lost_id == session_id)
    });
    drop(previous);
    if !lost.is_empty() {
        #[cfg(windows)]
        for (_, _, pid, _) in &lost {
            if let Some(pid) = pid {
                windows_process::release_child(*pid);
            }
        }
        let mut conn = db(&app)?;
        for (session_id, started_at, _, outcome) in lost {
            record_lost_session(&mut conn, session_id, now_wall, started_at, outcome)?;
        }
    }
    Ok(result)
}
#[tauri::command]
fn stop_game_session(app: tauri::AppHandle, session_id: i64) -> Result<String, String> {
    if session_id <= 0 {
        return Err("Session ID must be positive".into());
    }
    let connection = db(&app)?;
    let (pid, expected_start, build_path): (Option<i64>, Option<i64>, String) = connection
        .query_row(
            "SELECT sessions.process_pid,sessions.process_start_ticks,builds.path FROM sessions JOIN builds ON builds.id=sessions.build_id WHERE sessions.id=? AND sessions.ended_at IS NULL",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| "Active game session was not found".to_string())?;
    #[cfg(windows)]
    let pid = pid
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 1)
        .ok_or("The session has no verified emulator process ID")?;
    #[cfg(not(windows))]
    let pid = pid
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 1)
        .ok_or("The session has no verified emulator process ID")?;
    let expected_executable = Path::new(&build_path).join(emulator_binary_name());
    #[cfg(target_os = "linux")]
    stop_verified_session_group(pid, expected_start, &expected_executable)?;
    #[cfg(windows)]
    windows_process::terminate_verified(
        pid,
        expected_start.ok_or("The session has no verified process creation time")?,
        &expected_executable,
    )?;
    #[cfg(all(not(target_os = "linux"), not(windows)))]
    {
        let actual = proc_stat(pid as u32).ok_or("The emulator process has already exited")?;
        if !verified_session_process(
            i64::from(pid),
            expected_start,
            actual,
            process_executable_matches(pid as u32, &expected_executable),
        ) {
            return Err("The process identity or isolated process group no longer matches this registered emulator session".into());
        }
        stop_process_group(pid)?;
    }
    Ok(format!("Stop signal sent to emulator session {session_id}"))
}
#[tauri::command]
fn recent_sessions(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let c = db(&app)?;
    let mut s=c.prepare("SELECT sessions.id,sessions.game_id,sessions.build_id,games.title,sessions.started_at,sessions.ended_at,sessions.exit_code,sessions.outcome,sessions.log_path,builds.commit_sha,builds.build_type,sessions.process_pid FROM sessions JOIN games ON games.id=sessions.game_id JOIN builds ON builds.id=sessions.build_id ORDER BY sessions.id DESC LIMIT 10").map_err(|e|e.to_string())?;
    let rows=s.query_map([],|r|Ok(serde_json::json!({"id":r.get::<_,i64>(0)?,"game_id":r.get::<_,i64>(1)?,"build_id":r.get::<_,i64>(2)?,"title":r.get::<_,String>(3)?,"started_at":r.get::<_,i64>(4)?,"ended_at":r.get::<_,Option<i64>>(5)?,"exit_code":r.get::<_,Option<i32>>(6)?,"outcome":r.get::<_,Option<String>>(7)?,"log_path":r.get::<_,String>(8)?,"commit_sha":r.get::<_,String>(9)?,"build_type":r.get::<_,String>(10)?,"pid":r.get::<_,Option<i64>>(11)?}))).map_err(|e|e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}
#[tauri::command]
fn game_sessions(app: tauri::AppHandle, game_id: i64) -> Result<Vec<serde_json::Value>, String> {
    let c = db(&app)?;
    game(&c, game_id)?;
    let mut statement = c
        .prepare("SELECT sessions.id,sessions.game_id,sessions.build_id,games.title,sessions.started_at,sessions.ended_at,sessions.exit_code,sessions.outcome,sessions.log_path,builds.commit_sha,builds.build_type,sessions.process_pid FROM sessions JOIN games ON games.id=sessions.game_id JOIN builds ON builds.id=sessions.build_id WHERE sessions.game_id=? ORDER BY sessions.id DESC LIMIT 1000")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([game_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "game_id": row.get::<_, i64>(1)?,
                "build_id": row.get::<_, i64>(2)?,
                "title": row.get::<_, String>(3)?,
                "started_at": row.get::<_, i64>(4)?,
                "ended_at": row.get::<_, Option<i64>>(5)?,
                "exit_code": row.get::<_, Option<i32>>(6)?,
                "outcome": row.get::<_, Option<String>>(7)?,
                "log_path": row.get::<_, String>(8)?,
                "commit_sha": row.get::<_, String>(9)?,
                "build_type": row.get::<_, String>(10)?,
                "pid": row.get::<_, Option<i64>>(11)?
            }))
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}
#[tauri::command]
fn read_log(path: String, app: tauri::AppHandle) -> Result<String, String> {
    let base = fs::canonicalize(state_paths(&app)?.data.join("logs"))
        .map_err(|e| format!("Managed log directory is unavailable: {e}"))?;
    let p = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !path_starts_with(&p, &base) || !p.is_file() {
        return Err("Log path is outside the managed directory".into());
    }
    read_bounded_tail(&p, 4 * 1024 * 1024)
}
#[tauri::command]
fn export_log(path: String, destination: String, app: tauri::AppHandle) -> Result<(), String> {
    let log_root = fs::canonicalize(state_paths(&app)?.data.join("logs"))
        .map_err(|error| format!("Managed log directory is unavailable: {error}"))?;
    let source = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !path_starts_with(&source, &log_root) || !source.is_file() {
        return Err("Log path is outside the managed log directory".into());
    }
    copy_file_atomically(&source, Path::new(&destination), 512 * 1024 * 1024)
}
#[tauri::command]
fn export_crash_report(destination: String, content: String) -> Result<(), String> {
    const REPORT_LIMIT: usize = 2 * 1024 * 1024;
    if content.len() > REPORT_LIMIT {
        return Err("Crash report is larger than 2 MiB".into());
    }
    let destination = Path::new(&destination);
    let name = destination.file_name().ok_or("Choose a report file name")?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("Report folder is unavailable: {error}"))?;
    if !parent.is_dir() {
        return Err("Report destination is not a folder".into());
    }
    let destination = parent.join(name);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        nonce
    ));
    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        replace_file(&temporary, &destination)
            .map_err(|error| format!("Could not finalize crash report: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn copy_file_atomically(source: &Path, destination: &Path, limit: u64) -> Result<(), String> {
    let source_size = fs::metadata(source)
        .map_err(|error| error.to_string())?
        .len();
    if source_size > limit {
        return Err(format!("Log is larger than the {limit}-byte export limit"));
    }
    let name = destination.file_name().ok_or("Choose a destination file")?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("Export folder is unavailable: {error}"))?;
    if !parent.is_dir() {
        return Err("Export destination parent is not a directory".into());
    }
    let destination = parent.join(name);
    let source = fs::canonicalize(source).map_err(|error| error.to_string())?;
    if fs::canonicalize(&destination)
        .ok()
        .is_some_and(|existing| existing == source)
    {
        return Err("Choose a new file outside the managed log".into());
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        nonce
    ));
    let result = (|| -> Result<(), String> {
        let input = fs::File::open(&source).map_err(|error| error.to_string())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        let copied = std::io::copy(&mut input.take(limit.saturating_add(1)), &mut output)
            .map_err(|error| error.to_string())?;
        if copied > limit {
            return Err(format!("Log grew beyond the {limit}-byte export limit"));
        }
        output.sync_all().map_err(|error| error.to_string())?;
        drop(output);
        replace_file(&temporary, &destination)
            .map_err(|error| format!("Could not finalize exported log: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn read_bounded_tail(path: &Path, limit: u64) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let start = length.saturating_sub(limit);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(length.saturating_sub(start) as usize);
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let body = String::from_utf8_lossy(&bytes);
    if start == 0 {
        Ok(body.into_owned())
    } else {
        Ok(format!(
            "[Earlier log content omitted; showing last {limit} bytes]\n{body}"
        ))
    }
}
#[tauri::command]
fn read_build_log(app: tauri::AppHandle, id: i64) -> Result<String, String> {
    let c = db(&app)?;
    let b = build(&c, id)?;
    let root = fs::canonicalize(&b.path).map_err(|e| e.to_string())?;
    let path = fs::canonicalize(root.join("build.log")).map_err(|e| e.to_string())?;
    if !path_starts_with(&path, &root) || !path.is_file() {
        return Err("Build log is outside the registered installation".into());
    }
    read_bounded_tail(&path, 4 * 1024 * 1024)
}

#[derive(Debug)]
struct DiscordSession {
    game_id: i64,
    title: String,
    title_id: Option<String>,
    started_at: i64,
}

fn discord_current_state(app: &tauri::AppHandle) -> Result<(bool, Option<DiscordSession>), String> {
    let conn = db(app)?;
    let enabled = conn
        .query_row(
            "SELECT value FROM settings WHERE key='discord_presence_enabled'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map(|value| value.as_deref() != Some("false"))
        .map_err(|e| e.to_string())?;
    let active = conn
        .query_row(
            "SELECT g.id,g.title,g.title_id,s.started_at FROM sessions s JOIN games g ON g.id=s.game_id WHERE s.ended_at IS NULL ORDER BY s.started_at DESC,s.id DESC LIMIT 1",
            [],
            |r| Ok(DiscordSession { game_id: r.get(0)?, title: r.get(1)?, title_id: r.get(2)?, started_at: r.get(3)? }),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok((enabled, active))
}

#[cfg(unix)]
type DiscordStream = UnixStream;
#[cfg(windows)]
type DiscordStream = fs::File;

#[cfg(unix)]
fn discord_ipc_candidates_from_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|dir| seen.insert(dir.clone()))
        .flat_map(|dir| (0..10).map(move |index| dir.join(format!("discord-ipc-{index}"))))
        .collect()
}

#[cfg(unix)]
fn discord_ipc_candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for variable in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Some(dir) = env::var_os(variable).filter(|dir| !dir.is_empty()) {
            dirs.push(PathBuf::from(dir));
        }
    }
    dirs.push(PathBuf::from("/tmp"));
    discord_ipc_candidates_from_dirs(dirs)
}

#[cfg(windows)]
fn discord_ipc_candidates() -> Vec<PathBuf> {
    (0..10)
        .map(|index| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{index}")))
        .collect()
}

fn discord_write_frame<W: Write>(stream: &mut W, op: u32, payload: &[u8]) -> std::io::Result<()> {
    let length = u32::try_from(payload.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Discord frame is too large",
        )
    })?;
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&op.to_le_bytes());
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(payload);
    stream.write_all(&frame)
}

fn discord_ready_frame(opcode: u32, payload: &[u8]) -> bool {
    opcode == 1
        && serde_json::from_slice::<serde_json::Value>(payload)
            .ok()
            .and_then(|message| message.get("evt")?.as_str().map(str::to_owned))
            .as_deref()
            == Some("READY")
}

const DISCORD_MAX_FRAME_BYTES: usize = 64 * 1024;

fn discord_process_buffered_frames<W: Write>(
    stream: &mut W,
    buffered: &mut Vec<u8>,
) -> std::io::Result<()> {
    let mut consumed = 0;
    while buffered.len().saturating_sub(consumed) >= 8 {
        let header = &buffered[consumed..consumed + 8];
        let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
        let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        if length > DISCORD_MAX_FRAME_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Discord IPC frame exceeded the supported size",
            ));
        }
        let frame_end = consumed + 8 + length;
        if buffered.len() < frame_end {
            break;
        }
        let payload = &buffered[consumed + 8..frame_end];
        match opcode {
            2 => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    "Discord closed the Rich Presence connection",
                ));
            }
            3 => discord_write_frame(stream, 4, payload)?,
            1 | 4 => {}
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Discord IPC sent an unsupported opcode",
                ));
            }
        }
        consumed = frame_end;
    }
    if consumed > 0 {
        buffered.drain(..consumed);
    }
    if buffered.len() > DISCORD_MAX_FRAME_BYTES + 8 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Discord IPC receive buffer exceeded the supported size",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn discord_poll_frames(stream: &mut DiscordStream, buffered: &mut Vec<u8>) -> std::io::Result<()> {
    let mut chunk = [0_u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Discord IPC connection closed",
                ));
            }
            Ok(count) => buffered.extend_from_slice(&chunk[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    discord_process_buffered_frames(stream, buffered)
}

#[cfg(windows)]
fn discord_poll_frames(stream: &mut DiscordStream, buffered: &mut Vec<u8>) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;

    loop {
        let mut available = 0_u32;
        let ok = unsafe {
            PeekNamedPipe(
                stream.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if available == 0 {
            break;
        }
        let mut chunk = vec![0_u8; available.min(16 * 1024) as usize];
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Discord IPC connection closed",
            ));
        }
        buffered.extend_from_slice(&chunk[..count]);
    }
    discord_process_buffered_frames(stream, buffered)
}

#[cfg(unix)]
fn connect_discord(client_id: &str) -> Option<DiscordStream> {
    let mut stream = discord_ipc_candidates()
        .into_iter()
        .find_map(|path| UnixStream::connect(path).ok())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let handshake = serde_json::json!({"v": 1, "client_id": client_id});
    discord_write_frame(&mut stream, 0, handshake.to_string().as_bytes()).ok()?;
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).ok()?;
    let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
    if length > 64 * 1024 {
        return None;
    }
    let mut response = vec![0u8; length];
    stream.read_exact(&mut response).ok()?;
    if !discord_ready_frame(u32::from_le_bytes(header[..4].try_into().ok()?), &response) {
        return None;
    }
    stream.set_nonblocking(true).ok()?;
    Some(stream)
}

#[cfg(windows)]
fn connect_discord_at(client_id: &str, path: &Path) -> Option<DiscordStream> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;

    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // Opening a busy synchronous named pipe can otherwise wait forever. Discord
    // usually has a free instance; if it does not, skip it and try the next IPC
    // slot rather than blocking the presence worker.
    if unsafe { WaitNamedPipeW(path_wide.as_ptr(), 50) } == 0 {
        return None;
    }
    let mut stream = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .ok()?;
    let handshake = serde_json::json!({"v": 1, "client_id": client_id});
    discord_write_frame(&mut stream, 0, handshake.to_string().as_bytes()).ok()?;
    read_windows_discord_ready(&mut stream).then_some(stream)
}

#[cfg(windows)]
fn connect_discord(client_id: &str) -> Option<DiscordStream> {
    discord_ipc_candidates()
        .iter()
        .find_map(|path| connect_discord_at(client_id, path))
}

#[cfg(windows)]
fn read_windows_discord_ready(stream: &mut DiscordStream) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut header = [0_u8; 8];
    loop {
        let mut available = 0_u32;
        let ok = unsafe {
            PeekNamedPipe(
                stream.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || Instant::now() >= deadline {
            return false;
        }
        if available >= header.len() as u32 {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    if stream.read_exact(&mut header).is_err() {
        return false;
    }
    let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    if length > 64 * 1024 {
        return false;
    }
    while Instant::now() < deadline {
        let mut available = 0_u32;
        let ok = unsafe {
            PeekNamedPipe(
                stream.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return false;
        }
        if available >= length as u32 {
            let mut response = vec![0_u8; length];
            if stream.read_exact(&mut response).is_err() {
                return false;
            }
            return discord_ready_frame(
                u32::from_le_bytes(header[..4].try_into().unwrap()),
                &response,
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn discord_game_artwork_url(title_id: &str) -> Option<String> {
    let title_id = title_id
        .trim()
        .strip_suffix("_00")
        .unwrap_or(title_id.trim());
    if title_id.is_empty()
        || title_id.len() > 32
        || !title_id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .new_agent();
    for (country, language) in [("de", "de"), ("us", "en")] {
        let url = format!(
            "https://store.playstation.com/store/api/chihiro/00_09_000/titlecontainer/{country}/{language}/999/{title_id}_00"
        );
        let response = agent
            .get(&url)
            .header("User-Agent", "KytyPS5-Studio")
            .call();
        let Ok(mut response) = response else {
            continue;
        };
        let mut bytes = Vec::new();
        if response
            .body_mut()
            .as_reader()
            .take(2 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        let Ok(metadata) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let Some(images) = metadata.get("images").and_then(|value| value.as_array()) else {
            continue;
        };
        let Some(image) = images
            .iter()
            .find(|image| image.get("type").and_then(|value| value.as_i64()) == Some(10))
            .or_else(|| images.first())
        else {
            continue;
        };
        let Some(image_url) = image.get("url").and_then(|value| value.as_str()) else {
            continue;
        };
        if image_url.starts_with("https://") {
            return Some(image_url.to_owned());
        }
    }
    None
}

fn discord_activity(
    session: Option<&DiscordSession>,
    artwork_url: Option<&str>,
) -> serde_json::Value {
    let mut activity = serde_json::json!({"name": "KytyPS5 Studio", "type": 0});
    if let Some(session) = session {
        let title: String = session.title.chars().take(120).collect();
        activity["name"] = serde_json::json!(title);
        activity["details"] = serde_json::json!("Playing with KytyPS5 Studio");
        activity["state"] = serde_json::json!("KytyPS5 Studio");
        activity["timestamps"] =
            serde_json::json!({"start": session.started_at.saturating_mul(1000)});
        if let Some(image_url) = artwork_url {
            activity["assets"] = serde_json::json!({
                "large_image": image_url,
                "large_text": title,
            });
        }
    } else {
        activity["details"] = serde_json::json!("Browsing KytyPS5 Studio");
        activity["state"] = serde_json::json!("No game running");
    }
    activity
}

fn start_discord_presence(app: tauri::AppHandle) {
    thread::spawn(move || {
        let mut stream: Option<DiscordStream> = None;
        let mut incoming = Vec::new();
        let mut nonce = 0u64;
        let mut artwork_cache: HashMap<i64, Option<String>> = HashMap::new();
        loop {
            let (enabled, session) = discord_current_state(&app).unwrap_or_default();
            if !enabled {
                stream = None;
                incoming.clear();
                thread::sleep(Duration::from_secs(5));
                continue;
            }
            if stream.is_none() {
                stream = connect_discord("1561813805229674627");
            }
            if let Some(connection) = stream.as_mut() {
                let artwork_url = session.as_ref().and_then(|session| {
                    artwork_cache
                        .entry(session.game_id)
                        .or_insert_with(|| {
                            session
                                .title_id
                                .as_deref()
                                .and_then(discord_game_artwork_url)
                        })
                        .as_deref()
                });
                nonce = nonce.wrapping_add(1);
                let payload = serde_json::json!({
                    "cmd": "SET_ACTIVITY",
                    "args": {"pid": std::process::id(), "activity": discord_activity(session.as_ref(), artwork_url)},
                    "nonce": nonce.to_string()
                });
                let write_failed =
                    discord_write_frame(connection, 1, payload.to_string().as_bytes()).is_err();
                if write_failed {
                    stream = None;
                    incoming.clear();
                }
            }
            for _ in 0..15 {
                thread::sleep(Duration::from_secs(1));
                let poll_failed = stream.as_mut().is_some_and(|connection| {
                    discord_poll_frames(connection, &mut incoming).is_err()
                });
                if poll_failed {
                    stream = None;
                    incoming.clear();
                    break;
                }
            }
        }
    });
}

#[tauri::command]
fn get_setting(app: tauri::AppHandle, key: String) -> Result<Option<String>, String> {
    if ![
        "update_interval",
        "auto_update_mode",
        "theme",
        "discord_presence_enabled",
    ]
    .contains(&key.as_str())
        && !valid_shader_precompile_setting_key(&key)
    {
        return Err("Unsupported setting".into());
    }
    db(&app)?
        .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn set_setting(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    if ![
        "update_interval",
        "auto_update_mode",
        "theme",
        "discord_presence_enabled",
    ]
    .contains(&key.as_str())
        && !valid_shader_precompile_setting_key(&key)
    {
        return Err("Unsupported setting".into());
    }
    if key == "update_interval" && !valid_update_interval(&value) {
        return Err("Update interval must be startup-only, 6 hours, 24 hours, or weekly".into());
    }
    if key == "auto_update_mode" && !valid_auto_update_mode(&value) {
        return Err("Update behavior must be notify or build".into());
    }
    if key == "discord_presence_enabled" && !["true", "false"].contains(&value.as_str()) {
        return Err("Discord presence must be enabled or disabled".into());
    }
    if valid_shader_precompile_setting_key(&key) && !["true", "false"].contains(&value.as_str()) {
        return Err("Shader precompile setting must be enabled or disabled".into());
    }
    db(&app)?.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(|e|e.to_string())?;
    Ok(())
}
fn valid_shader_precompile_setting_key(key: &str) -> bool {
    let Some(ids) = key.strip_prefix("shader_precompile_enabled:") else {
        return false;
    };
    let parsed = ids
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>();
    parsed.is_ok_and(|values| {
        (values.len() == 1 || values.len() == 2) && values.iter().all(|id| *id > 0)
    })
}
fn shader_precompile_setting_key(build_id: i64) -> String {
    format!("shader_precompile_enabled:{build_id}")
}
fn get_build_shader_precompile_enabled(conn: &Connection, build_id: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key=?",
        [shader_precompile_setting_key(build_id)],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map(|value| match value.as_deref() {
        Some("true") => true,
        Some("false") => false,
        _ => cfg!(windows),
    })
    .map_err(|error| error.to_string())
}
fn get_game_shader_precompile_enabled(
    conn: &Connection,
    game_id: i64,
    build_id: i64,
) -> Result<bool, String> {
    let game_key = format!("shader_precompile_enabled:{build_id}:{game_id}");
    let game_value = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [game_key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match game_value.as_deref() {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => get_build_shader_precompile_enabled(conn, build_id),
    }
}
fn valid_update_interval(value: &str) -> bool {
    matches!(value, "0" | "6" | "24" | "168")
}
fn valid_auto_update_mode(value: &str) -> bool {
    matches!(value, "notify" | "build")
}
fn notification_text(value: &str, maximum: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(maximum)
        .collect()
}
#[tauri::command]
fn desktop_notification(app: tauri::AppHandle, title: String, body: String) -> Result<(), String> {
    let title = notification_text(&title, 120);
    let body = notification_text(&body, 2048);
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| format!("Desktop notifications are unavailable: {error}"))
}
#[cfg(any(target_os = "linux", test))]
fn should_disable_dmabuf_renderer(
    session_type: &str,
    user_override_present: bool,
    drm_vendors: &[String],
) -> bool {
    session_type == "wayland"
        && !user_override_present
        && drm_vendors.iter().any(|vendor| vendor == "0x10de")
}
use rusqlite::OptionalExtension;
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        let session_type = env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let vendor_ids: Vec<_> = fs::read_dir("/sys/class/drm")
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path().join("device/vendor")).ok())
            .map(|vendor| vendor.trim().to_string())
            .collect();
        if should_disable_dmabuf_renderer(
            &session_type,
            env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some(),
            &vendor_ids,
        ) {
            env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
    let p = Paths::new().expect("Cannot create application directories");
    p.db().expect("Cannot initialize database");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            start_discord_presence(app.handle().clone());
            Ok(())
        })
        .manage(p)
        .manage(Arc::new(Jobs {
            pid: AtomicI32::new(0),
            active: AtomicBool::new(false),
            updating: AtomicBool::new(false),
            job_id: AtomicU64::new(0),
            event_kind: Mutex::new(String::new()),
            cancel_requested: AtomicBool::new(false),
            cancellable: AtomicBool::new(false),
            control: Mutex::new(()),
        }))
        .manage(ProcessSamples::default())
        .invoke_handler(tauri::generate_handler![
            check_app_update,
            install_app_update,
            list_repos,
            open_repository_folder,
            list_builds,
            open_build_folder,
            validate_build,
            list_games,
            shader_precompile_support,
            precompile_game_shaders,
            remove_game_entry,
            game_compatibility,
            official_compatibility_index,
            official_compatibility_details,
            record_compatibility_observation,
            compatibility_observations,
            add_repository,
            discover_forks,
            github_repository_details,
            list_community_pull_requests,
            community_pull_request_details,
            start_community_build,
            community_build_history,
            rename_community_build_config,
            save_github_token,
            delete_github_token,
            github_token_status,
            inspect_build_source,
            system_info,
            start_install_build_tools,
            storage_report,
            clean_incomplete_builds,
            clean_compiler_cache,
            clean_community_worktrees,
            clean_session_logs,
            inspect_graphics,
            start_clone,
            inspect_repository,
            check_updates,
            start_build,
            start_update_build,
            cancel_job,
            activate_build,
            remove_build,
            add_game,
            remove_repository,
            refresh_game_metadata,
            open_game_folder,
            read_game_icon,
            read_game_logo,
            read_game_background,
            scan_games,
            set_game_build,
            toggle_favorite,
            get_profile,
            inspect_game_content,
            fetch_game_cheats,
            download_game_cheat,
            import_game_patch,
            save_custom_game_patch,
            set_game_patch_mod_enabled,
            get_profile_capabilities,
            set_profile,
            get_global_profile_options,
            get_active_profile_capabilities,
            set_global_profile_options,
            get_build_profile_capabilities,
            get_build_profile_options,
            set_build_profile_options,
            add_game_to_steam,
            launch_game,
            launch_game_ab,
            process_metrics,
            stop_game_session,
            recent_sessions,
            game_sessions,
            read_log,
            export_log,
            export_crash_report,
            read_build_log,
            get_setting,
            set_setting,
            desktop_notification,
            save_data::list_saves,
            save_data::backup_save,
            save_data::backup_save_with_trophies,
            save_data::list_save_backups,
            save_data::set_save_backup_directory,
            save_data::clean_save_backups,
            save_data::restore_save,
            save_data::import_save_directory,
            save_data::import_save_archive,
            save_data::export_save_archive,
            save_data::delete_save,
            save_data::open_save_folder,
            trophies::list_trophy_progress,
            trophies::backup_trophy_progress,
            trophies::export_trophy_progress,
            trophies::import_trophy_progress,
            trophies::open_trophy_folder,
            trophies::list_trophy_backups,
            trophies::restore_trophy_backup
        ])
        .run(tauri::generate_context!())
        .expect("Tauri error");
}
#[cfg(test)]
mod tests {
    fn verified_profile_capabilities(help: &str) -> ProfileCapabilities {
        let mut capabilities = profile_capabilities(help);
        capabilities.verified_options = capabilities.options.clone();
        for flag in &capabilities.verified_options {
            capabilities.option_states.insert(
                flag.clone(),
                CapabilityStatus {
                    state: CapabilityState::VerifiedSupported,
                    explanation: "test source proof".into(),
                },
            );
        }
        capabilities
    }

    #[test]
    fn shader_precompile_progress_counts_completed_pipeline_shards_and_live_workers() {
        let root = std::env::temp_dir().join(format!(
            "kyty-shader-progress-test-{}-{}",
            std::process::id(),
            super::now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("plan-recorded.out.log"),
            "Shader seeds: plan saved to plan.bin (200 pipelines, 120 of them this seed file's)\n",
        )
        .unwrap();
        std::fs::write(
            root.join("plan-seeds.out.log"),
            "Shader seeds: plan saved to plan.bin (500 pipelines, 400 of them this seed file's)\nShader seeds: 30 programs (0 indirect image tables)\n",
        )
        .unwrap();
        std::fs::write(
            root.join("shard0of10.out.log"),
            "Static pipeline binaries: saved 17 pipelines (17 captured)\nShader precompile: complete\n",
        )
        .unwrap();
        std::fs::write(
            root.join("shard1of10.out.log"),
            "Static pipeline binaries: saved 23 pipelines (23 captured)\n",
        )
        .unwrap();

        let progress = super::read_shader_precompile_progress(&root);
        assert_eq!(progress.shaders_total, Some(30));
        assert_eq!(progress.pipelines_compiled, 17);
        assert_eq!(progress.pipelines_total, Some(520));
        assert_eq!(progress.live_workers, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn game_patch_import_requires_matching_title_and_version_and_installs_enabled_flags_in_order() {
        let game = super::Game {
            id: 7,
            path: "/games/astro".into(),
            title: "ASTRO BOT".into(),
            favorite: false,
            build_id: None,
            play_seconds: 0,
            last_played: None,
            title_id: Some("PPSA21567".into()),
            game_version: Some("01.019.000".into()),
            icon_path: None,
            install_size: None,
            file_status: "Available".into(),
        };
        let source = br#"{"id":"PPSA21567","version":"01.019.000","process":"eboot.bin","mods":[{"name":"First","enabled":true,"memory":[]},{"name":"Second","enabled":false,"memory":[]}] }"#;
        let mut patch = super::validate_game_patch_document(&game, source).unwrap();
        super::set_patch_mod_enabled(&mut patch, 1, true).unwrap();
        let mods = patch["mods"].as_array().unwrap();
        assert_eq!(mods[0]["name"], "First");
        assert_eq!(mods[1]["name"], "Second");
        assert_eq!(mods[1]["enabled"], true);

        let wrong_version = br#"{"id":"PPSA21567","version":"01.018.000","process":"eboot.bin","mods":[{"name":"Fix","memory":[]}] }"#;
        assert!(super::validate_game_patch_document(&game, wrong_version)
            .unwrap_err()
            .contains("version 01.018.000"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_containment_uses_case_folding_and_component_boundaries() {
        use std::path::Path;

        assert!(super::path_starts_with(
            Path::new(r"\\?\C:\Users\Älice\Games\Title"),
            Path::new(r"\\?\c:\users\äLICE\games")
        ));
        assert!(!super::path_starts_with(
            Path::new(r"\\?\C:\Users\Alice2\Games"),
            Path::new(r"\\?\C:\Users\Alice")
        ));
        assert!(super::path_starts_with(
            Path::new(r"\\?\UNC\Server\Share\Games\Title"),
            Path::new(r"\\?\UNC\server\share\games")
        ));
        assert!(super::paths_equal(
            Path::new(r"C:\Users\Alice\Games"),
            Path::new(r"c:\users\alice\games")
        ));
    }
    #[cfg(not(windows))]
    #[test]
    fn path_containment_keeps_linux_case_sensitive_semantics() {
        use std::path::Path;

        assert!(super::path_starts_with(
            Path::new("/home/alice/games/title"),
            Path::new("/home/alice/games")
        ));
        assert!(!super::path_starts_with(
            Path::new("/home/alice/Games/title"),
            Path::new("/home/alice/games")
        ));
        assert!(!super::path_starts_with(
            Path::new("/home/alice/games-backup"),
            Path::new("/home/alice/games")
        ));
    }
    #[test]
    fn compatibility_summary_selects_each_hosts_report_and_falls_back_to_overall() {
        let game = super::OfficialCompatibilityGame {
            title_id: "PPSA12345".into(),
            title: "Example".into(),
            overall: "main-menu".into(),
            status: "main-menu".into(),
            preferred_os: "all".into(),
            os: HashMap::from([
                ("linux".into(), "playable".into()),
                ("Windows".into(), "logo".into()),
            ]),
            report_counts: HashMap::new(),
            latest_tested: None,
        };
        let linux = super::prioritize_compatibility_game(game.clone(), "linux");
        assert_eq!(linux.preferred_os, "linux");
        assert_eq!(linux.status, "playable");
        let windows = super::prioritize_compatibility_game(game.clone(), "windows");
        assert_eq!(windows.preferred_os, "windows");
        assert_eq!(windows.status, "logo");
        let fallback = super::prioritize_compatibility_game(game, "macos");
        assert_eq!(fallback.preferred_os, "all");
        assert_eq!(fallback.status, "main-menu");

        let cached = serde_json::json!([{
            "title_id": "PPSA12345",
            "title": "Example",
            "overall": "main-menu",
            "status": "playable",
            "preferred_os": "linux",
            "os": { "linux": "playable", "windows": "logo" },
            "report_counts": {},
            "latest_tested": null
        }]);
        let reloaded = super::cached_compatibility_games(cached, "windows");
        assert_eq!(reloaded[0].preferred_os, "windows");
        assert_eq!(reloaded[0].status, "logo");
    }
    #[test]
    fn official_compatibility_index_decodes_site_camel_case_and_prioritizes_host_report() {
        let payload = serde_json::json!({
            "version": 2,
            "games": [{
                "key": "PPSA12345",
                "title": "Example",
                "titleId": "PPSA12345",
                "overall": "in-game",
                "os": {"windows": "main-menu", "linux": "in-game"},
                "reportCounts": {"windows": 1, "linux": 1},
                "latestTested": "2026-10-20",
                "cover": "https://example.invalid/cover.png"
            }]
        });
        let index: super::OfficialSiteIndexPayload =
            serde_json::from_value(payload).expect("current official index schema decodes");
        let game = &index.games[0];
        let prioritized = super::prioritize_compatibility_game(
            super::OfficialCompatibilityGame {
                title_id: game.title_id.clone().unwrap(),
                title: game.title.clone(),
                overall: game.overall.clone(),
                status: game.overall.clone(),
                preferred_os: "all".into(),
                os: game.os.clone(),
                report_counts: game.report_counts.clone(),
                latest_tested: game.latest_tested.clone(),
            },
            "windows",
        );
        assert_eq!(prioritized.title_id, "PPSA12345");
        assert_eq!(prioritized.preferred_os, "windows");
        assert_eq!(prioritized.status, "main-menu");
        assert_eq!(prioritized.report_counts["windows"], 1);
    }

    use super::*;

    #[test]
    fn configured_storage_roots_must_be_absolute_paths() {
        assert_eq!(
            absolute_env_path(Some(std::ffi::OsString::from("relative/data"))),
            None
        );
        let absolute = std::env::temp_dir().join("kyty-storage-root");
        assert_eq!(
            absolute_env_path(Some(absolute.as_os_str().to_os_string())),
            Some(absolute)
        );
        assert_eq!(absolute_env_path(None), None);
    }
    #[cfg(windows)]
    fn windows_test_command(program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args);
        prepare_owned_child(&mut command);
        command
    }

    #[test]
    fn atomic_file_replacement_overwrites_existing_unicode_path() {
        let root = std::env::temp_dir().join(format!(
            "kyty-atomic-replace-{}-{}-ü",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("shortcuts-ゲーム.vdf");
        let temporary = root.join("shortcuts-ゲーム.vdf.tmp");
        fs::write(&destination, b"old data").unwrap();
        fs::write(&temporary, b"new data").unwrap();

        replace_file(&temporary, &destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"new data");
        assert!(!temporary.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(any(windows, test))]
    #[test]
    fn windows_steam_launch_arguments_preserve_unicode_unc_and_quoted_paths() {
        assert_eq!(
            windows_command_line_quote(r"C:\Games\PS5 Titles\"),
            r#""C:\Games\PS5 Titles\\""#
        );
        assert_eq!(
            windows_command_line_quote(r"\\server\profile share\ゲーム\kyty_emulator.exe"),
            r#""\\server\profile share\ゲーム\kyty_emulator.exe""#
        );
        assert_eq!(
            windows_command_line_quote(r#"D:\保存したゲーム\a"b\"#),
            r#""D:\保存したゲーム\a\"b\\""#
        );
        assert_eq!(windows_command_line_quote("a\"b"), r#""a\"b""#);
    }
    #[test]
    fn steam_windows_paths_strip_verbatim_prefix_for_short_paths_only() {
        assert_eq!(
            steam_windows_path(Path::new(r"\\?\C:\Games\Title")),
            r"C:\Games\Title"
        );
        assert_eq!(
            steam_windows_path(Path::new(r"\\?\UNC\server\share\Title")),
            r"\\server\share\Title"
        );

        let long_path = format!(r"\\?\C:\{}", "x".repeat(260));
        assert_eq!(steam_windows_path(Path::new(&long_path)), long_path);
    }
    #[test]
    fn steam_shortcuts_vdf_empty_root_accepts_a_new_entry_without_extra_terminators() {
        let mut shortcuts = b"\0shortcuts\0\0\x08\x08".to_vec();
        let (root_end, max_index) = steam_root_end(&shortcuts).unwrap();
        assert_eq!(max_index, 0);
        assert_eq!(steam_next_entry_index(root_end, max_index).unwrap(), 0);
        assert_eq!(&shortcuts[root_end..], b"\x08\x08");

        let mut entry = vec![0];
        entry.extend_from_slice(b"0\0");
        steam_vdf_string(&mut entry, "AppName", "Studio test").unwrap();
        entry.push(8);
        shortcuts.splice(root_end..root_end, entry);

        let (root_end, max_index) = steam_root_end(&shortcuts).unwrap();
        assert_eq!(max_index, 0);
        assert_eq!(steam_next_entry_index(root_end, max_index).unwrap(), 1);
        assert_eq!(&shortcuts[root_end..], b"\x08\x08");
    }
    #[test]
    fn steam_shortcut_index_overflow_is_reported_without_reusing_an_entry() {
        let mut shortcuts = b"\0shortcuts\0\0".to_vec();
        shortcuts.push(0);
        shortcuts.extend_from_slice(b"4294967295\0");
        shortcuts.extend_from_slice(b"\x08\x08\x08");
        let (root_end, max_index) = steam_root_end(&shortcuts).unwrap();
        assert_eq!(max_index, u32::MAX);
        assert_eq!(
            steam_next_entry_index(root_end, max_index).unwrap_err(),
            "Steam shortcuts.vdf has no available entry index"
        );
    }
    #[test]
    fn steam_shortcut_upsert_updates_build_and_collapses_duplicates() {
        let mut shortcuts = b"\0shortcuts\0\0\x08\x08".to_vec();
        let (root_end, _) = steam_root_end(&shortcuts).unwrap();
        let make_entry = |exe: &str, options: &str| {
            let mut entry = vec![0];
            entry.extend_from_slice(b"0\0");
            steam_vdf_string(&mut entry, "AppName", "Game · KytyPS5 Studio").unwrap();
            steam_vdf_string(&mut entry, "Exe", exe).unwrap();
            steam_vdf_string(&mut entry, "LaunchOptions", options).unwrap();
            entry.push(8);
            entry
        };
        let first = make_entry("old-emulator.exe", "old-game-args");
        shortcuts.splice(root_end..root_end, first.clone());
        shortcuts.splice(root_end..root_end, first);
        let updated = make_entry("new-emulator.exe", "new-game-args");
        steam_upsert_shortcut(&mut shortcuts, "Game · KytyPS5 Studio", &updated).unwrap();

        let app_name = "Game · KytyPS5 Studio".as_bytes();
        assert_eq!(
            shortcuts
                .windows(app_name.len())
                .filter(|part| *part == app_name)
                .count(),
            1
        );
        assert!(shortcuts
            .windows(b"new-emulator.exe".len())
            .any(|part| part == b"new-emulator.exe"));
        assert!(!shortcuts
            .windows(b"old-emulator.exe".len())
            .any(|part| part == b"old-emulator.exe"));
    }
    #[test]
    fn steam_shortcut_vdf_strings_reject_nuls_and_preserve_unicode() {
        let mut output = Vec::new();
        steam_vdf_string(&mut output, "AppName", "KytyPS5 · ゲーム 🎮").unwrap();
        assert!(output
            .windows("ゲーム 🎮".len())
            .any(|part| part == "ゲーム 🎮".as_bytes()));

        let before = output.clone();
        assert!(steam_vdf_string(&mut output, "AppName", "bad\0title")
            .unwrap_err()
            .contains("NUL"));
        assert_eq!(
            output, before,
            "rejected strings do not mutate the VDF buffer"
        );
        assert!(steam_vdf_string(&mut output, "bad\0key", "value").is_err());
    }
    #[test]
    fn steam_shortcuts_vdf_rejects_excessive_object_nesting() {
        let mut nested = Vec::new();
        for _ in 0..=STEAM_VDF_MAX_NESTING {
            nested.extend_from_slice(b"\0nested\0");
        }
        nested.resize(nested.len() + STEAM_VDF_MAX_NESTING + 1, 8);

        assert!(steam_vdf_skip(&nested, 0, 0)
            .unwrap_err()
            .contains("nesting depth"));
    }
    #[cfg(unix)]
    #[test]
    fn discord_ipc_search_uses_standard_temp_fallbacks_in_order_without_duplicates() {
        let candidates = discord_ipc_candidates_from_dirs([
            PathBuf::from("/run/user/1000"),
            PathBuf::from("/custom/tmp"),
            PathBuf::from("/custom/tmp"),
            PathBuf::from("/tmp"),
        ]);
        assert_eq!(candidates.len(), 30);
        assert_eq!(candidates[0], PathBuf::from("/run/user/1000/discord-ipc-0"));
        assert_eq!(candidates[10], PathBuf::from("/custom/tmp/discord-ipc-0"));
        assert_eq!(candidates[20], PathBuf::from("/tmp/discord-ipc-0"));
    }
    #[test]
    fn discord_rpc_frames_are_written_as_one_buffer() {
        #[derive(Default)]
        struct RecordingWriter {
            calls: usize,
            bytes: Vec<u8>,
        }
        impl Write for RecordingWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.calls += 1;
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut writer = RecordingWriter::default();
        discord_write_frame(&mut writer, 1, b"payload").unwrap();

        assert_eq!(writer.calls, 1);
        assert_eq!(&writer.bytes[..4], &1_u32.to_le_bytes());
        assert_eq!(&writer.bytes[4..8], &7_u32.to_le_bytes());
        assert_eq!(&writer.bytes[8..], b"payload");
    }
    #[test]
    fn discord_handshake_requires_the_ready_opcode_and_event() {
        let ready = br#"{"evt":"READY"}"#;
        assert!(discord_ready_frame(1, ready));
        assert!(!discord_ready_frame(0, ready));
        assert!(!discord_ready_frame(1, br#"{"evt":"ERROR"}"#));
        assert!(!discord_ready_frame(1, b"not JSON"));
    }
    #[test]
    fn discord_rpc_answers_ping_after_receiving_a_complete_frame() {
        let payload = b"ping-payload";
        let mut incoming = Vec::new();
        incoming.extend_from_slice(&3_u32.to_le_bytes());
        incoming.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        incoming.extend_from_slice(payload);
        let mut writer = Vec::new();

        discord_process_buffered_frames(&mut writer, &mut incoming).unwrap();

        assert!(incoming.is_empty());
        assert_eq!(&writer[..4], &4_u32.to_le_bytes());
        assert_eq!(&writer[4..8], &(payload.len() as u32).to_le_bytes());
        assert_eq!(&writer[8..], payload);
    }
    #[test]
    fn discord_rpc_keeps_partial_frames_until_the_payload_arrives() {
        let mut frame = Vec::new();
        frame.extend_from_slice(&3_u32.to_le_bytes());
        frame.extend_from_slice(&4_u32.to_le_bytes());
        frame.extend_from_slice(b"pong");
        let mut incoming = frame[..10].to_vec();
        let mut writer = Vec::new();

        discord_process_buffered_frames(&mut writer, &mut incoming).unwrap();

        assert!(writer.is_empty());
        assert_eq!(incoming, frame[..10]);
        incoming.extend_from_slice(&frame[10..]);
        discord_process_buffered_frames(&mut writer, &mut incoming).unwrap();

        assert!(incoming.is_empty());
        assert_eq!(&writer[..4], &4_u32.to_le_bytes());
        assert_eq!(&writer[8..], b"pong");
    }
    #[cfg(windows)]
    #[test]
    fn windows_discord_client_handshakes_over_a_real_named_pipe() {
        use std::{
            io::Read,
            os::windows::io::{FromRawHandle, RawHandle},
            sync::mpsc,
        };
        use windows_sys::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW};

        let pipe_name = format!(r"\\.\pipe\kytyps5-studio-test-{}", std::process::id());
        let pipe_wide = pipe_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let path = PathBuf::from(&pipe_name);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (ready_consumed_tx, ready_consumed_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let handle = unsafe {
                CreateNamedPipeW(
                    pipe_wide.as_ptr(),
                    0x0000_0003,
                    0,
                    1,
                    4096,
                    4096,
                    2_000,
                    std::ptr::null(),
                )
            };
            assert!(!handle.is_null() && handle as isize != -1);
            let mut pipe = unsafe { fs::File::from_raw_handle(handle as RawHandle) };
            ready_tx.send(()).unwrap();
            let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) };
            assert!(connected != 0 || std::io::Error::last_os_error().raw_os_error() == Some(535));

            let mut header = [0_u8; 8];
            pipe.read_exact(&mut header).unwrap();
            assert_eq!(u32::from_le_bytes(header[..4].try_into().unwrap()), 0);
            let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
            let mut payload = vec![0; length];
            pipe.read_exact(&mut payload).unwrap();
            let handshake: serde_json::Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(handshake["v"], 1);
            assert_eq!(handshake["client_id"], "test-client");

            let ready = serde_json::json!({"evt": "READY"}).to_string();
            discord_write_frame(&mut pipe, 1, ready.as_bytes()).unwrap();
            // Wait until connect_discord_at has consumed READY before closing
            // the server end so the test covers a complete real-pipe handshake.
            ready_consumed_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("client consumed the Discord READY frame");
        });
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("fake Discord server created its named pipe");

        let connection =
            connect_discord_at("test-client", &path).expect("client accepts Discord READY frame");
        ready_consumed_tx
            .send(())
            .expect("fake Discord server waits for READY consumption");
        drop(connection);
        server
            .join()
            .expect("fake Discord server completed handshake");
    }
    #[cfg(unix)]
    #[test]
    fn unix_emulator_detection_keeps_the_linux_filename_and_execute_bit() {
        let root = std::env::temp_dir().join(format!(
            "kyty-emulator-path-{}-{}",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&root).unwrap();
        let executable = emulator_executable(&root);
        fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(emulator_binary_name(), "kyty_emulator");
        assert!(is_emulator_executable(&executable));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_emulator_executable(&executable));
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn windows_storage_paths_follow_local_and_roaming_appdata() {
        let paths = Paths::from_windows_roots(
            PathBuf::from(r"C:\Users\Jörg Test\AppData\Local"),
            PathBuf::from(r"C:\Users\Jörg Test\AppData\Roaming"),
        );
        assert_eq!(
            paths.data,
            PathBuf::from(r"C:\Users\Jörg Test\AppData\Local\kytyps5-studio")
        );
        assert_eq!(
            paths.cache,
            PathBuf::from(r"C:\Users\Jörg Test\AppData\Local\kytyps5-studio\Cache")
        );
        assert_eq!(
            paths.config,
            PathBuf::from(r"C:\Users\Jörg Test\AppData\Roaming\kytyps5-studio")
        );
        let unc = Paths::from_windows_roots(
            PathBuf::from(r"\\server\profile share\Local"),
            PathBuf::from(r"\\server\profile share\Roaming"),
        );
        assert!(unc.data.starts_with(r"\\server\profile share\Local"));
        assert!(unc.config.starts_with(r"\\server\profile share\Roaming"));
    }
    #[test]
    fn windows_job_pid_slot_round_trips_the_full_unsigned_pid_range() {
        for pid in [2_u32, i32::MAX as u32, i32::MAX as u32 + 1, u32::MAX] {
            assert_eq!(windows_pid_from_job_slot(pid as i32), Some(pid));
        }
        assert_eq!(windows_pid_from_job_slot(0), None);
    }
    #[cfg(windows)]
    #[test]
    fn windows_emulator_detection_requires_the_native_executable_suffix() {
        let root = std::env::temp_dir().join(format!(
            "kyty-emulator-path-{}-{}",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&root).unwrap();
        let executable = emulator_executable(&root);
        fs::write(&executable, b"test executable fixture").unwrap();
        assert_eq!(emulator_binary_name(), "kyty_emulator.exe");
        assert!(is_emulator_executable(&executable));
        let incorrect = root.join("kyty_emulator");
        fs::write(&incorrect, b"not a Windows executable").unwrap();
        assert!(!is_emulator_executable(&incorrect));
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn windows_profile_paths_preserve_unicode_and_spaces_when_canonicalized() {
        let root = std::env::temp_dir().join(format!(
            "KytyPS5 Studio 路徑-{}-{}",
            std::process::id(),
            now()
        ));
        let directory = root.join("遊戲資料");
        fs::create_dir_all(&directory).unwrap();
        let canonical_directory = fs::canonicalize(&directory).unwrap();

        let normalized_directory =
            canonicalize_option_path("--addcont-dir", directory.to_string_lossy().as_ref())
                .unwrap();
        assert_eq!(PathBuf::from(normalized_directory), canonical_directory);

        let output_file = directory.join("遊戲日誌.txt");
        let normalized_output = canonicalize_option_path(
            "--printf-output-file",
            output_file.to_string_lossy().as_ref(),
        )
        .unwrap();
        assert_eq!(
            PathBuf::from(normalized_output),
            canonical_directory.join("遊戲日誌.txt")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lost_session_completion_and_playtime_are_atomic_and_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE games(id INTEGER PRIMARY KEY,play_seconds INTEGER NOT NULL DEFAULT 0,last_played INTEGER);
                 CREATE TABLE sessions(id INTEGER PRIMARY KEY,game_id INTEGER NOT NULL,ended_at INTEGER,outcome TEXT);
                 INSERT INTO games(id,play_seconds) VALUES(1,15);
                 INSERT INTO sessions(id,game_id) VALUES(7,1),(8,1);",
            )
            .unwrap();

        assert!(record_lost_session(&mut connection, 7, 300, Some(100), "process_lost").unwrap());
        assert!(!record_lost_session(&mut connection, 7, 400, Some(100), "process_lost").unwrap());
        let game: (i64, Option<i64>) = connection
            .query_row(
                "SELECT play_seconds,last_played FROM games WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let session: (Option<i64>, Option<String>) = connection
            .query_row(
                "SELECT ended_at,outcome FROM sessions WHERE id=7",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(game, (215, Some(100)));
        assert_eq!(session, (Some(300), Some("process_lost".into())));

        connection
            .execute_batch(
                "CREATE TRIGGER reject_playtime BEFORE UPDATE OF play_seconds ON games BEGIN SELECT RAISE(ABORT,'test failure'); END;",
            )
            .unwrap();
        assert!(record_lost_session(&mut connection, 8, 500, Some(200), "process_lost").is_err());
        let uncommitted: Option<i64> = connection
            .query_row("SELECT ended_at FROM sessions WHERE id=8", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(uncommitted, None);
    }

    #[test]
    fn compiler_cache_cleanup_is_confined_to_managed_cache_and_reports_size() {
        let cache = env::temp_dir().join(format!("kyty-cache-test-{}", now()));
        let build_cache = cache.join("builds");
        fs::create_dir_all(&build_cache).unwrap();
        fs::write(build_cache.join("artifact.bin"), [0_u8; 17]).unwrap();
        assert_eq!(directory_size(&build_cache), 17);
        assert_eq!(
            clear_managed_compiler_cache(&build_cache, &cache).unwrap(),
            17
        );
        assert!(!build_cache.exists());

        let outside = cache.with_file_name(format!("kyty-outside-{}", now()));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), b"preserve").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &build_cache).unwrap();
            assert!(clear_managed_compiler_cache(&build_cache, &cache).is_err());
            assert!(outside.join("keep").exists());
            fs::remove_file(&build_cache).unwrap();
        }
        fs::remove_dir_all(cache).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn community_temporary_cleanup_reports_size_and_rejects_symlinks() {
        let cache = env::temp_dir().join(format!("kyty-temp-cache-{}", now()));
        let root = cache.join("community-worktrees");
        let worktree = root.join("run-1");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join("source.bin"), [0_u8; 31]).unwrap();
        assert_eq!(remove_managed_temp_directory(&worktree, &root).unwrap(), 31);
        assert!(!worktree.exists());

        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join("source.bin"), b"remove on scope exit").unwrap();
        {
            let _guard = ManagedTemporaryDirectory {
                path: worktree.clone(),
                root: root.clone(),
            };
        }
        assert!(!worktree.exists());

        let outside = cache.with_file_name(format!("kyty-temp-outside-{}", now()));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), b"preserve").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("external")).unwrap();
            assert!(remove_managed_temp_directory(&root.join("external"), &root).is_err());
            assert!(outside.join("keep").exists());
        }
        fs::remove_dir_all(cache).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn incomplete_build_cleanup_only_removes_unregistered_partial_installations() {
        let root = env::temp_dir().join(format!("kyty-build-install-cleanup-{}", now()));
        let repo = root.join("7");
        let partial = repo.join("0123456789ab-Release-123");
        let existing_executable = repo.join("0123456789ab-Release-124");
        let registered = repo.join("0123456789ab-Debug-125");
        let unrelated = repo.join("notes");
        let outside = root.with_file_name(format!("kyty-build-cleanup-external-{}", now()));
        fs::create_dir_all(&partial).unwrap();
        fs::create_dir_all(&existing_executable).unwrap();
        fs::create_dir_all(&registered).unwrap();
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(partial.join("partial-file"), b"partial").unwrap();
        fs::write(
            existing_executable.join(emulator_binary_name()),
            b"preserve",
        )
        .unwrap();
        fs::write(registered.join("partial-file"), b"tracked").unwrap();
        fs::write(unrelated.join("file"), b"preserve").unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("preserve"), b"external").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, repo.join("0123456789ab-Release-126")).unwrap();
        let registered_paths = HashSet::from([fs::canonicalize(&registered).unwrap()]);

        assert_eq!(
            incomplete_build_installations(&root, &registered_paths),
            vec![fs::canonicalize(&partial).unwrap()]
        );
        assert_eq!(
            clean_incomplete_build_installations(&root, &registered_paths).unwrap(),
            b"partial".len() as u64
        );
        assert!(!partial.exists());
        assert!(existing_executable.join(emulator_binary_name()).exists());
        assert!(registered.join("partial-file").exists());
        assert!(unrelated.join("file").exists());
        assert!(outside.join("preserve").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn session_log_cleanup_preserves_history_and_unmanaged_paths() {
        let data = env::temp_dir().join(format!("kyty-session-cleanup-{}", now()));
        let logs = data.join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("old.log"), [0_u8; 23]).unwrap();
        let outside = data.with_file_name(format!("kyty-external-log-{}", now()));
        fs::write(&outside, b"keep this log").unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE sessions(id INTEGER PRIMARY KEY,log_path TEXT NOT NULL);
                 INSERT INTO sessions VALUES(1,'PLACEHOLDER');
                 INSERT INTO sessions VALUES(2,'EXTERNAL');",
            )
            .unwrap();
        connection
            .execute(
                "UPDATE sessions SET log_path=? WHERE id=1",
                [logs.join("old.log").display().to_string()],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE sessions SET log_path=? WHERE id=2",
                [outside.display().to_string()],
            )
            .unwrap();

        assert_eq!(
            clear_managed_session_logs(&mut connection, &logs, &data).unwrap(),
            23
        );
        assert!(logs.is_dir());
        assert_eq!(directory_size(&logs), 0);
        let managed_path: String = connection
            .query_row("SELECT log_path FROM sessions WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let external_path: String = connection
            .query_row("SELECT log_path FROM sessions WHERE id=2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(managed_path.is_empty());
        assert_eq!(external_path, outside.display().to_string());
        assert_eq!(fs::read(&outside).unwrap(), b"keep this log");
        fs::remove_dir_all(data).unwrap();
        fs::remove_file(outside).unwrap();
    }
    #[test]
    fn repository_pr_search_is_encoded_and_scoped_to_repo_and_state() {
        let url = github_pull_request_search_url(
            "owner",
            "kyty",
            "closed",
            "night mode & logging",
            3,
            20,
        );
        let parsed = url::Url::parse(&url).unwrap();
        let query: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            query.get("q").unwrap(),
            "repo:owner/kyty is:pr state:closed night mode & logging"
        );
        assert_eq!(query.get("page").unwrap(), "3");
        assert_eq!(query.get("per_page").unwrap(), "20");
    }
    #[test]
    fn github_search_pagination_stops_at_the_api_result_limit() {
        assert!(github_search_has_more(49, 20, 1_500));
        assert!(!github_search_has_more(50, 20, 1_500));
        assert!(github_search_has_more(2, 20, 250));
    }
    #[test]
    fn eboot_filename_matching_follows_host_filesystem_case_rules() {
        let lowercase = std::ffi::OsStr::new("eboot.bin");
        let uppercase = std::ffi::OsStr::new("EBOOT.BIN");
        assert!(is_eboot_filename_for(lowercase, false));
        assert!(!is_eboot_filename_for(uppercase, false));
        assert!(is_eboot_filename_for(lowercase, true));
        assert!(is_eboot_filename_for(uppercase, true));
    }
    #[test]
    fn session_logs_have_a_hard_size_limit_and_mark_truncated_output() {
        let path = env::temp_dir().join(format!("kyty-session-log-{}", now()));
        let mut log = SessionLogWriter::new(fs::File::create(&path).unwrap(), 256);
        log.append_line("stdout", &"x".repeat(500)).unwrap();
        log.append_line("stderr", "this should not be appended")
            .unwrap();
        drop(log);
        let contents = fs::read(&path).unwrap();
        assert!(contents.len() <= 256);
        assert!(String::from_utf8_lossy(&contents).contains("further output was omitted"));
        assert!(!String::from_utf8_lossy(&contents).contains("this should not be appended"));
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn build_logs_have_a_hard_limit_across_build_phases() {
        let path = env::temp_dir().join(format!("kyty-build-log-{}", now()));
        {
            let file = fs::File::create(&path).unwrap();
            let mut log = BuildLogWriter::new(file, 256);
            log.append_line("command", "cmake configure").unwrap();
            log.append_line("stdout", &"x".repeat(500)).unwrap();
            log.append_line("stderr", "this should not be appended")
                .unwrap();
        }
        let first_contents = fs::read(&path).unwrap();
        assert!(first_contents.len() <= 256);
        assert!(
            String::from_utf8_lossy(&first_contents).contains("Build log reached its 32 MiB limit")
        );
        {
            let file = fs::OpenOptions::new().append(true).open(&path).unwrap();
            let mut next_phase = BuildLogWriter::new(file, 256);
            next_phase
                .append_line("stdout", "later CMake stage output")
                .unwrap();
        }
        assert_eq!(fs::read(&path).unwrap(), first_contents);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn process_output_reader_caps_single_lines_and_keeps_following_output() {
        let input = format!("{}\nnext line\nlast", "x".repeat(1024));
        let mut reader =
            CappedOutputReader::with_limit(std::io::Cursor::new(input.into_bytes()), 32);
        let first = reader.next_line().unwrap().unwrap();
        assert_eq!(first.bytes.len(), 32);
        assert!(first.truncated);
        let second = reader.next_line().unwrap().unwrap();
        assert_eq!(second.text(), "next line");
        let third = reader.next_line().unwrap().unwrap();
        assert_eq!(third.text(), "last");
        assert!(reader.next_line().unwrap().is_none());
    }
    #[test]
    fn git_output_capture_is_bounded_and_still_drains_the_pipe() {
        let contents = b"0123456789";
        let mut input = std::io::Cursor::new(contents);
        let (captured, truncated) = read_capped_output(&mut input, 4).unwrap();
        assert_eq!(captured, b"0123");
        assert!(truncated);
        assert_eq!(input.position(), contents.len() as u64);

        let (complete, was_truncated) = read_capped_output(&contents[..], 10).unwrap();
        assert_eq!(complete, contents);
        assert!(!was_truncated);
    }
    fn write_zarchive_fixture(path: &Path) {
        let footer = 65_618;
        let total_size = footer + 144;
        let mut bytes = vec![0_u8; total_size];
        bytes[65_576..65_586].copy_from_slice(b"\x09eboot.bin");
        bytes[65_586..65_590].copy_from_slice(&0x7fff_ffff_u32.to_be_bytes());
        bytes[65_590..65_594].copy_from_slice(&1_u32.to_be_bytes());
        bytes[65_594..65_598].copy_from_slice(&1_u32.to_be_bytes());
        bytes[65_602..65_606].copy_from_slice(&0x8000_0000_u32.to_be_bytes());
        let sections = [
            (0_u64, 65_536_u64),
            (65_536, 40),
            (65_576, 10),
            (65_586, 32),
            (65_618, 0),
            (65_618, 0),
        ];
        for (index, (offset, size)) in sections.into_iter().enumerate() {
            let start = footer + index * 16;
            bytes[start..start + 8].copy_from_slice(&offset.to_be_bytes());
            bytes[start + 8..start + 16].copy_from_slice(&size.to_be_bytes());
        }
        bytes[footer + 128..footer + 136].copy_from_slice(&(total_size as u64).to_be_bytes());
        bytes[footer + 136..footer + 140].copy_from_slice(&0x61bf_3a01_u32.to_be_bytes());
        bytes[footer + 140..footer + 144].copy_from_slice(&0x169f_52d6_u32.to_be_bytes());
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn game_size_calculation_counts_regular_files_and_ignores_symlinks() {
        let root = env::temp_dir().join(format!(
            "kyty-game-size-test-{}-{}",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.bin"), b"abc").unwrap();
        fs::write(root.join("b.bin"), b"de").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("a.bin"), root.join("linked.bin")).unwrap();
        assert_eq!(calculate_game_size(&root), Some(5));
        assert_eq!(calculate_game_size(&root.join("a.bin")), Some(3));
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn recursive_game_scan_identifies_symlink_entries_for_rejection() {
        let root = env::temp_dir().join(format!(
            "kyty-game-scan-symlink-{}-{}",
            std::process::id(),
            now()
        ));
        let outside = root.with_file_name(format!("kyty-outside-game-{}", now()));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("eboot.bin"), b"\x7fELF\x02\x01").unwrap();
        std::os::unix::fs::symlink(outside.join("eboot.bin"), root.join("game.elf")).unwrap();

        let entry = walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(Result::ok)
            .find(|entry| entry.path() == root.join("game.elf"))
            .unwrap();
        assert!(scan_entry_is_symlink(&entry));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn game_file_status_reports_removed_paths_without_touching_game_data() {
        let root = env::temp_dir().join(format!(
            "kyty-game-status-test-{}-{}",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&root).unwrap();
        let game = root.join("game.zar");
        fs::write(&game, b"existing").unwrap();
        assert_eq!(game_file_status(&game), "Available");
        fs::remove_file(&game).unwrap();
        assert_eq!(game_file_status(&game), "Missing");
        fs::remove_dir_all(root).unwrap();
    }

    fn test_git(path: &Path, args: &[&str]) -> Result<String, String> {
        git(Some(path), args)
    }
    #[test]
    fn community_merge_is_repeatable_order_sensitive_and_reports_conflicts() {
        let root = env::temp_dir().join(format!(
            "kyty-pr-merge-test-{}-{}",
            std::process::id(),
            now()
        ));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        test_git(&source, &["init", "-b", "main"]).unwrap();
        test_git(&source, &["config", "user.name", "Test"]).unwrap();
        test_git(&source, &["config", "user.email", "test@localhost"]).unwrap();
        fs::write(source.join("common.txt"), "base\n").unwrap();
        test_git(&source, &["add", "."]).unwrap();
        test_git(
            &source,
            &["-c", "commit.gpgsign=false", "commit", "-m", "base"],
        )
        .unwrap();
        let base = test_git(&source, &["rev-parse", "HEAD"]).unwrap();
        for (branch, file, content) in [("pr1", "one.txt", "one"), ("pr2", "two.txt", "two")] {
            test_git(&source, &["checkout", "-b", branch, &base]).unwrap();
            fs::write(source.join(file), format!("{content}\n")).unwrap();
            test_git(&source, &["add", "."]).unwrap();
            test_git(
                &source,
                &["-c", "commit.gpgsign=false", "commit", "-m", branch],
            )
            .unwrap();
        }
        for (branch, content) in [("conflict1", "left"), ("conflict2", "right")] {
            test_git(&source, &["checkout", "-b", branch, &base]).unwrap();
            fs::write(source.join("common.txt"), format!("{content}\n")).unwrap();
            test_git(&source, &["add", "."]).unwrap();
            test_git(
                &source,
                &["-c", "commit.gpgsign=false", "commit", "-m", branch],
            )
            .unwrap();
        }
        let integrate = |name: &str, branches: &[&str]| {
            let path = root.join(name);
            let source_text = source.to_string_lossy().to_string();
            let path_text = path.to_string_lossy().to_string();
            git(
                None,
                &[
                    "clone",
                    "--no-hardlinks",
                    "--no-checkout",
                    &source_text,
                    &path_text,
                ],
            )
            .unwrap();
            test_git(&path, &["config", "user.name", "KytyPS5 Studio"]).unwrap();
            test_git(&path, &["config", "user.email", "studio@localhost"]).unwrap();
            test_git(&path, &["checkout", "--detach", &base]).unwrap();
            for branch in branches {
                git_deterministic_merge_test(&path, &format!("refs/remotes/origin/{branch}"))
                    .unwrap();
            }
            (
                path.clone(),
                test_git(&path, &["rev-parse", "HEAD"]).unwrap(),
            )
        };
        let (_, first) = integrate("first", &["pr1", "pr2"]);
        let (_, repeated) = integrate("repeated", &["pr1", "pr2"]);
        let (reordered, alternate) = integrate("reordered", &["pr2", "pr1"]);
        assert_eq!(
            first, repeated,
            "same base, PR heads, options, and order produce the same combined commit"
        );
        assert_ne!(first, alternate, "merge order is part of the result");
        let conflict = root.join("conflict");
        let source_text = source.to_string_lossy().to_string();
        let conflict_text = conflict.to_string_lossy().to_string();
        git(
            None,
            &[
                "clone",
                "--no-hardlinks",
                "--no-checkout",
                &source_text,
                &conflict_text,
            ],
        )
        .unwrap();
        test_git(&conflict, &["config", "user.name", "KytyPS5 Studio"]).unwrap();
        test_git(&conflict, &["config", "user.email", "studio@localhost"]).unwrap();
        test_git(&conflict, &["checkout", "--detach", &base]).unwrap();
        git_deterministic_merge_test(&conflict, "refs/remotes/origin/conflict1").unwrap();
        assert!(git_deterministic_merge_test(&conflict, "refs/remotes/origin/conflict2").is_err());
        let files = test_git(&conflict, &["diff", "--name-only", "--diff-filter=U"]).unwrap();
        assert_eq!(files, "common.txt");
        fs::remove_dir_all(root).unwrap();
        let _ = reordered;
    }

    #[test]
    fn community_build_signature_covers_repository_order_and_build_options() {
        let pull_requests = vec![(101, "a".repeat(40)), (115, "b".repeat(40))];
        let signature = community_build_signature(
            7,
            "main",
            &"c".repeat(40),
            &pull_requests,
            "Release",
            8,
            "clang",
            false,
        );
        assert_eq!(
            signature,
            community_build_signature(
                7,
                "main",
                &"c".repeat(40),
                &pull_requests,
                "Release",
                8,
                "clang",
                false,
            )
        );
        assert_ne!(
            signature,
            community_build_signature(
                8,
                "main",
                &"c".repeat(40),
                &pull_requests,
                "Release",
                8,
                "clang",
                false,
            )
        );
        assert_ne!(
            signature,
            community_build_signature(
                7,
                "main",
                &"c".repeat(40),
                &pull_requests.iter().cloned().rev().collect::<Vec<_>>(),
                "Release",
                8,
                "clang",
                false,
            )
        );
        assert_ne!(
            signature,
            community_build_signature(
                7,
                "main",
                &"c".repeat(40),
                &pull_requests,
                "Debug",
                8,
                "clang",
                false,
            )
        );
    }
    #[test]
    fn desktop_notification_text_is_bounded_and_strips_controls() {
        assert_eq!(notification_text("bad\nvalue\u{7} ok", 120), "badvalue ok");
        assert_eq!(notification_text("abcdef", 3), "abc");
    }

    #[cfg(unix)]
    #[test]
    fn process_group_termination_stops_the_requested_child_group() {
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        assert_eq!(unsafe { libc::getpgid(pid) }, pid);
        let waiter = thread::spawn(move || child.wait().unwrap());
        stop_process_group(pid).unwrap();
        assert!(!waiter.join().unwrap().success());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn verified_session_stop_pins_the_leader_before_signaling_its_group() {
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        let executable = fs::canonicalize(format!("/proc/{pid}/exe")).unwrap();
        let start_ticks = proc_stat(pid as u32).unwrap().start_ticks as i64;
        stop_verified_session_group(pid, Some(start_ticks), &executable).unwrap();
        assert!(!child.wait().unwrap().success());
        assert!(!process_group_exists(pid));
    }

    #[cfg(unix)]
    #[test]
    fn process_group_termination_escalates_to_bounded_forced_kill() {
        let mut child = Command::new("python3")
            .args([
                "-c",
                "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); print('ready', flush=True); time.sleep(30)",
            ])
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        let waiter = thread::spawn(move || child.wait().unwrap());
        let started = Instant::now();
        stop_process_group(pid).unwrap();
        assert!(started.elapsed() >= Duration::from_secs(2));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!waiter.join().unwrap().success());
    }

    #[cfg(unix)]
    #[test]
    fn bounded_child_wait_cleans_the_process_group_on_timeout() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30 & wait"])
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        let started = Instant::now();
        let result =
            wait_for_child_with_timeout(&mut child, Duration::from_millis(100), "test command");
        assert!(result.unwrap_err().contains("time limit"));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!process_group_exists(pid));
    }

    #[cfg(windows)]
    #[test]
    fn windows_owned_child_does_not_run_before_job_assignment() {
        let mut command = Command::new("cmd.exe");
        command
            .args(["/d", "/c", "exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        prepare_owned_child(&mut command);
        let mut child = command
            .spawn()
            .expect("Windows command processor is available");

        thread::sleep(Duration::from_millis(150));
        assert!(
            child.try_wait().unwrap().is_none(),
            "a managed Windows child must remain suspended before job assignment"
        );
        attach_owned_child(&mut child).expect("child joins its cleanup job and resumes");
        let status = wait_for_child_with_timeout(
            &mut child,
            Duration::from_secs(5),
            "suspended-child regression test",
        )
        .expect("resumed child exits successfully");
        assert!(status.success(), "resumed child exited with {status}");
    }

    #[cfg(windows)]
    #[test]
    fn bounded_windows_child_wait_kills_and_reaps_the_owned_job_on_timeout() {
        let mut child = windows_test_command(
            "cmd.exe",
            &["/d", "/c", "cmd.exe /d /c ping -n 30 127.0.0.1 >NUL"],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("Windows command processor is available");
        let pid = child.id();
        attach_owned_child(&mut child).expect("timeout child joins its cleanup job");

        let started = Instant::now();
        let result =
            wait_for_child_with_timeout(&mut child, Duration::from_millis(100), "test command");

        assert!(result.unwrap_err().contains("time limit"));
        assert!(started.elapsed() < Duration::from_secs(25));
        assert!(
            child.try_wait().unwrap().is_some(),
            "timed out child is reaped"
        );
        assert!(
            windows_process::snapshot(pid).is_none_or(|snapshot| !snapshot.running),
            "timed out process no longer has a live resource snapshot"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_timeout_does_not_block_when_the_job_registry_is_unavailable() {
        let mut child = windows_test_command(
            "cmd.exe",
            &["/d", "/c", "cmd.exe /d /c ping -n 30 127.0.0.1 >NUL"],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("Windows command processor is available");
        let pid = child.id();
        attach_owned_child(&mut child).expect("timeout child joins its cleanup job");
        let retained_job = windows_process::take_job_for_test(pid)
            .expect("test can simulate a lost job registry entry");

        let started = Instant::now();
        let result =
            wait_for_child_with_timeout(&mut child, Duration::from_millis(100), "test command");

        let error = result.expect_err("missing job registry entry must report cleanup failure");
        assert!(error.contains("No owned Windows process job"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(15));
        assert!(
            child.try_wait().unwrap().is_none(),
            "the simulated process is still live"
        );
        child.kill().expect("test process can be stopped directly");
        let _ = child.wait();
        drop(retained_job);
    }

    #[cfg(windows)]
    #[test]
    fn windows_job_cancellation_stops_the_owned_process_and_sets_cancel_state() {
        let mut child = windows_test_command(
            "cmd.exe",
            &["/d", "/c", "cmd.exe /d /c ping -n 30 127.0.0.1 >NUL"],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("Windows command processor is available");
        let pid = child.id() as i32;
        attach_owned_child(&mut child).expect("child joins its cleanup job");
        let jobs = Jobs {
            pid: AtomicI32::new(pid),
            active: AtomicBool::new(true),
            updating: AtomicBool::new(false),
            job_id: AtomicU64::new(1),
            event_kind: Mutex::new("build".into()),
            cancel_requested: AtomicBool::new(false),
            cancellable: AtomicBool::new(true),
            control: Mutex::new(()),
        };

        request_job_cancellation(&jobs).expect("active cancellable job should stop");
        let status = wait_for_child_with_timeout(
            &mut child,
            Duration::from_secs(2),
            "cancelled test command",
        )
        .expect("cancelled child is reaped");

        assert!(jobs.cancel_requested.load(Ordering::SeqCst));
        assert!(
            !status.success(),
            "cancelled child should not exit successfully"
        );
        assert!(
            windows_process::snapshot(pid as u32).is_none_or(|snapshot| !snapshot.running),
            "cancelled process no longer has a live resource snapshot"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_owned_job_kills_descendants_after_the_root_exits() {
        let pid_file = env::temp_dir().join(format!(
            "kytyps5-studio-child-{}-{}.txt",
            std::process::id(),
            now()
        ));
        let _ = fs::remove_file(&pid_file);
        let script = "$child = Start-Process -FilePath 'cmd.exe' -ArgumentList '/d /c ping -n 30 127.0.0.1 >NUL' -PassThru; [System.IO.File]::WriteAllText($env:KYTY_TEST_CHILD_PID_FILE, [string]$child.Id)";
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env("KYTY_TEST_CHILD_PID_FILE", &pid_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        prepare_owned_child(&mut command);
        let mut child = command.spawn().expect("Windows PowerShell is available");
        attach_owned_child(&mut child).expect("root process joins its cleanup job");

        let started = Instant::now();
        let (status, cleanup) = wait_for_child_and_cleanup_group(&mut child, || {}).unwrap();
        let descendant_pid = fs::read_to_string(&pid_file)
            .expect("PowerShell recorded the descendant PID")
            .trim()
            .parse::<u32>()
            .expect("PowerShell recorded a valid descendant PID");
        let _ = fs::remove_file(&pid_file);

        assert!(status.success(), "PowerShell exited with {status}");
        assert!(cleanup.is_ok(), "descendant cleanup failed: {cleanup:?}");
        assert!(started.elapsed() < Duration::from_secs(25));
        assert!(
            windows_process::snapshot(descendant_pid).is_none_or(|snapshot| !snapshot.running),
            "descendant process survived cleanup of its owned job"
        );
    }

    #[cfg(unix)]
    #[test]
    fn exited_emulator_group_cleans_descendants_that_hold_output_pipes() {
        use std::io::Read;
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30 & exit 0"])
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pgid = child.id() as i32;
        let (status, cleanup) = wait_for_child_and_cleanup_group(&mut child, || {}).unwrap();
        assert!(status.success());
        assert!(cleanup.is_ok(), "{cleanup:?}");
        assert!(!process_group_exists(pgid));
        let mut stdout = child.stdout.take().unwrap();
        let mut remaining = Vec::new();
        stdout.read_to_end(&mut remaining).unwrap();
    }

    #[test]
    fn migration_preserves_existing_records_and_adds_metadata_columns() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE repositories(id INTEGER PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, url TEXT NOT NULL UNIQUE, branch TEXT NOT NULL, source_path TEXT, adapter TEXT NOT NULL DEFAULT 'unknown', installed_commit TEXT);
             CREATE TABLE builds(id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repositories(id), commit_sha TEXT NOT NULL, build_type TEXT NOT NULL, path TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL, active INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE games(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL, favorite INTEGER NOT NULL DEFAULT 0, build_id INTEGER REFERENCES builds(id), play_seconds INTEGER NOT NULL DEFAULT 0, last_played INTEGER);
             CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE sessions(id INTEGER PRIMARY KEY, game_id INTEGER NOT NULL REFERENCES games(id), build_id INTEGER NOT NULL REFERENCES builds(id), started_at INTEGER NOT NULL, ended_at INTEGER, exit_code INTEGER, outcome TEXT, log_path TEXT NOT NULL);
             CREATE TABLE profiles(game_id INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE, screen_width INTEGER, screen_height INTEGER, present_mode TEXT, perf_overlay INTEGER NOT NULL DEFAULT 0);
             INSERT INTO repositories VALUES(1, 'KytyPS5', 'KytyPS5', 'https://github.com/KytyPS5/KytyPS5', 'master', '/source', 'official', 'abc123');
             INSERT INTO builds VALUES(1, 1, 'abc123', 'Release', '/build', 1234, 1);
             INSERT INTO games VALUES(1, '/game', 'Existing Game', 1, 1, 90, 1234);
             INSERT INTO settings VALUES('theme', 'dark');
             INSERT INTO sessions VALUES(1, 1, 1, 1200, 1300, 0, 'normal', '/logs/game.log');
             INSERT INTO profiles VALUES(1, 1920, 1080, 'Fifo', 1);"
        ).unwrap();

        migrate(&mut db).unwrap();

        let repository: String = db
            .query_row("SELECT name FROM repositories WHERE id=1", [], |r| r.get(0))
            .unwrap();
        let build_path: String = db
            .query_row("SELECT path FROM builds WHERE id=1 AND active=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let game_title: String = db
            .query_row(
                "SELECT title FROM games WHERE id=1 AND favorite=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let setting: String = db
            .query_row("SELECT value FROM settings WHERE key='theme'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let session: (i64, Option<i32>, String, String) = db
            .query_row(
                "SELECT started_at,exit_code,outcome,log_path FROM sessions WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        let profile: (i64, i64, String, i64) = db
            .query_row(
                "SELECT screen_width,screen_height,present_mode,perf_overlay FROM profiles WHERE game_id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        let version: i64 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        let columns: i64 = db.query_row("SELECT count(*) FROM pragma_table_info('games') WHERE name IN ('title_id','game_version','icon_path','install_size')", [], |r| r.get(0)).unwrap();
        let hidden_default: i64 = db
            .query_row("SELECT hidden FROM games WHERE id=1", [], |r| r.get(0))
            .unwrap();
        let compatibility_table: i64 = db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='manual_compatibility_observations'", [], |r| r.get(0)).unwrap();
        let profile_columns: i64 = db.query_row("SELECT count(*) FROM pragma_table_info('profiles') WHERE name IN ('controller_color','controller_volume','controller_vibration','keyboard_player')", [], |r| r.get(0)).unwrap();
        let profile_options_default: String = db
            .query_row(
                "SELECT options_json FROM profiles WHERE game_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let process_columns: i64 = db.query_row("SELECT count(*) FROM pragma_table_info('sessions') WHERE name IN ('process_pid','process_start_ticks')", [], |r| r.get(0)).unwrap();
        let build_columns: i64 = db.query_row("SELECT count(*) FROM pragma_table_info('builds') WHERE name IN ('install_size_bytes','validation_status','configuration_json')", [], |r| r.get(0)).unwrap();
        let build_configuration: String = db
            .query_row(
                "SELECT configuration_json FROM builds WHERE id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let build_validation: String = db
            .query_row("SELECT validation_status FROM builds WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            (
                repository.as_str(),
                build_path.as_str(),
                game_title.as_str(),
                setting.as_str()
            ),
            ("KytyPS5", "/build", "Existing Game", "dark")
        );
        assert_eq!(version, 19);
        assert_eq!(hidden_default, 0);
        assert_eq!(compatibility_table, 1);
        assert_eq!(columns, 4);
        assert_eq!(profile_columns, 4);
        assert_eq!(profile_options_default, "{}");
        assert_eq!(process_columns, 2);
        assert_eq!(build_columns, 3);
        assert_eq!(build_configuration, "{}");
        assert_eq!(build_validation, "not-checked");
        let community_tables: i64 = db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('community_pr_cache','community_build_configs','community_build_prs','community_build_results')",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(community_tables, 4);
        assert_eq!(
            session,
            (1200, Some(0), "normal".into(), "/logs/game.log".into())
        );
        let fullscreen: i64 = db
            .query_row("SELECT fullscreen FROM profiles WHERE game_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(profile, (1920, 1080, "Fifo".into(), 1));
        assert_eq!(fullscreen, 0);
    }
    #[test]
    fn version_four_profile_migration_preserves_settings_and_adds_fullscreen_default() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE profiles(game_id INTEGER PRIMARY KEY, screen_width INTEGER, screen_height INTEGER, present_mode TEXT, perf_overlay INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE builds(id INTEGER PRIMARY KEY, repo_id INTEGER, commit_sha TEXT, build_type TEXT, path TEXT, created_at INTEGER, active INTEGER);
             INSERT INTO profiles VALUES(7, 2560, 1440, 'Mailbox', 1);
             PRAGMA user_version=4;",
        )
        .unwrap();
        migrate(&mut db).unwrap();
        let row: (i64, i64, String, i64, i64) = db
            .query_row(
                "SELECT screen_width,screen_height,present_mode,perf_overlay,fullscreen FROM profiles WHERE game_id=7",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        let version: i64 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(row, (2560, 1440, "Mailbox".into(), 1, 0));
        assert_eq!(version, 19);
    }
    #[test]
    fn migration_removes_only_the_legacy_sqlite_github_token() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE builds(id INTEGER PRIMARY KEY, active INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE global_profile_defaults(id INTEGER PRIMARY KEY, options_json TEXT NOT NULL);
             CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO settings VALUES('github_token','legacy-secret'),('theme','dark');
             PRAGMA user_version=9;",
        )
        .unwrap();
        migrate(&mut db).unwrap();
        let legacy_token: i64 = db
            .query_row(
                "SELECT count(*) FROM settings WHERE key='github_token'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let theme: String = db
            .query_row("SELECT value FROM settings WHERE key='theme'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(legacy_token, 0);
        assert_eq!(theme, "dark");
    }
    #[test]
    fn reads_game_metadata_from_the_upstream_param_layout() {
        let dir = std::env::temp_dir().join(format!("kyty-game-metadata-{}", now()));
        let sce_sys = dir.join("sce_sys");
        fs::create_dir_all(&sce_sys).unwrap();
        fs::write(sce_sys.join("param.json"), r#"{"titleId":"PPSA12345","appVersion":"01.02","localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"Sample Game"}}}"#).unwrap();
        fs::write(sce_sys.join("icon0.png"), b"test").unwrap();
        let (title, title_id, version, icon) = game_metadata(&dir);
        assert_eq!(title, "Sample Game");
        assert_eq!(title_id.as_deref(), Some("PPSA12345"));
        assert_eq!(version.as_deref(), Some("01.02"));
        assert_eq!(
            icon.as_deref(),
            Some(sce_sys.join("icon0.png").to_string_lossy().as_ref())
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn game_artwork_reads_only_small_png_files_within_the_game_directory() {
        let dir = std::env::temp_dir().join(format!("kyty-artwork-{}", now()));
        let root = dir.join("game");
        fs::create_dir_all(root.join("sce_sys")).unwrap();
        let image = root.join("sce_sys/pic0.png");
        let bytes = b"\x89PNG\r\n\x1a\nart";
        fs::write(&image, bytes).unwrap();
        assert_eq!(read_game_png(&image, &root, 64), Some(bytes.to_vec()));
        assert!(read_game_png(&image, &root, 4).is_none());
        let outside = dir.join("outside.png");
        fs::write(&outside, bytes).unwrap();
        assert!(read_game_png(&outside, &root, 64).is_none());
        #[cfg(unix)]
        {
            let linked = root.join("sce_sys/linked.png");
            std::os::unix::fs::symlink(&outside, &linked).unwrap();
            assert!(read_game_png(&linked, &root, 64).is_none());
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn game_artwork_is_downscaled_before_it_is_returned_to_the_renderer() {
        let mut source_pixels = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                if x < 2 {
                    source_pixels.extend_from_slice(&[255, 0, 0, 255]);
                } else {
                    source_pixels.extend_from_slice(&[0, 0, 255, 255]);
                }
            }
        }
        let mut source_png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut source_png, 4, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&source_pixels).unwrap();
        }

        let thumbnail = resize_png(&source_png, 2, 2).unwrap();
        let mut reader = png::Decoder::new(Cursor::new(&thumbnail))
            .read_info()
            .unwrap();
        let mut output = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut output).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert!(thumbnail.len() < source_png.len() + 64);
        assert_eq!(&output[..4], &[255, 0, 0, 255]);
        assert_eq!(&output[4..8], &[0, 0, 255, 255]);
        assert!(resize_png(&source_png, 0, 2).is_none());
    }

    #[test]
    fn profile_values_reject_unsupported_ranges_and_malformed_colors() {
        let mut profile = Profile {
            controller_color: Some("#a1B2c3".into()),
            controller_volume: Some(100),
            controller_vibration: Some(0),
            keyboard_player: Some(4),
            ..Profile::default()
        };
        assert!(validate_profile_values(&profile).is_ok());
        profile.screen_width = Some(1);
        profile.screen_height = Some(u32::MAX);
        assert!(validate_profile_values(&profile).is_ok());
        profile.screen_width = Some(0);
        assert!(validate_profile_values(&profile).is_err());
        profile.screen_width = None;
        profile.screen_height = None;
        profile.controller_color = Some("#１２３４５６".into());
        assert!(validate_profile_values(&profile).is_err());
        profile.controller_color = None;
        profile.controller_volume = Some(101);
        assert!(validate_profile_values(&profile).is_err());
        profile.controller_volume = None;
        profile.keyboard_player = Some(5);
        assert!(validate_profile_values(&profile).is_err());
    }
    #[test]
    fn gpu_and_vblank_values_match_upstream_ranges() {
        let capabilities = verified_profile_capabilities(
            "--gpu <index> Vulkan device\n--vblank-frequency <num> Virtual frequency",
        );
        for (flag, value) in [
            ("--gpu", "-1"),
            ("--gpu", "0"),
            ("--vblank-frequency", "30"),
            ("--vblank-frequency", "360"),
        ] {
            assert!(validate_extra_profile_options(
                &BTreeMap::from([(flag.to_string(), value.to_string())]),
                &capabilities,
            )
            .is_ok());
        }
        for (flag, value) in [
            ("--gpu", "-2"),
            ("--vblank-frequency", "29"),
            ("--vblank-frequency", "361"),
        ] {
            assert!(validate_extra_profile_options(
                &BTreeMap::from([(flag.to_string(), value.to_string())]),
                &capabilities,
            )
            .is_err());
        }
    }
    #[test]
    fn explicit_gpu_selection_must_exist_in_vulkan_diagnostics() {
        let automatic = BTreeMap::from([("--gpu".into(), "-1".into())]);
        let explicit = BTreeMap::from([("--gpu".into(), "1".into())]);
        assert!(validate_gpu_hardware_selection(&automatic, "").is_ok());
        assert!(validate_gpu_hardware_selection(
            &explicit,
            "GPU0:\n  deviceName = GPU A\nGPU 1:\n"
        )
        .is_ok());
        assert!(
            validate_gpu_hardware_selection(&explicit, "GPU0:\n  deviceName = GPU A\n").is_err()
        );
    }
    #[test]
    fn help_advertisement_alone_does_not_authorize_an_option() {
        let capabilities = profile_capabilities("--future-switch Toggle future behavior\n");
        assert!(capabilities
            .options
            .iter()
            .any(|flag| flag == "--future-switch"));
        assert!(capabilities.verified_options.is_empty());
        assert!(matches!(
            capabilities.option_states["--future-switch"].state,
            CapabilityState::AdvertisedUnverified
        ));
        assert!(validate_extra_profile_options(
            &BTreeMap::from([("--future-switch".into(), "true".into())]),
            &capabilities
        )
        .is_err());
    }
    #[test]
    fn trophy_duration_is_hidden_by_the_correct_disable_switch_and_rejected_if_combined() {
        let capabilities = verified_profile_capabilities(
            "--no-trophy-notifications Disable trophy notifications\n--trophy-duration <seconds> Notification duration\n",
        );
        assert!(capabilities
            .switch_options
            .contains(&"--no-trophy-notifications".into()));
        let invalid = BTreeMap::from([
            ("--no-trophy-notifications".into(), "true".into()),
            ("--trophy-duration".into(), "7".into()),
        ]);
        assert!(validate_extra_profile_options(&invalid, &capabilities).is_err());
        assert_eq!(
            effective_profile_options(&invalid, &BTreeMap::new(), &capabilities).unwrap(),
            BTreeMap::from([("--no-trophy-notifications".into(), "true".into())])
        );
    }
    #[test]
    fn profile_capabilities_are_derived_from_emulator_help() {
        let capabilities = profile_capabilities(
            "Options:\n  --screen-width <num> Window width\n  --screen-height <num> Window height\n  --present-mode <value> Fifo\n  --perf-overlay <true|false> Performance overlay\n  --fullscreen Run fullscreen\n  --controller-color <#RRGGBB> Controller light\n  --controller-volume <0-100> Speaker volume\n  --controller-vibration <0-100> Vibration\n  --keyboard-player <0-4> Keyboard player\n",
        );
        assert!(capabilities.screen_width);
        assert!(capabilities.screen_height);
        assert!(capabilities.present_mode);
        assert!(capabilities.perf_overlay);
        assert!(capabilities.fullscreen);
        assert!(capabilities.controller_color);
        assert!(capabilities.controller_volume);
        assert!(capabilities.controller_vibration);
        assert!(capabilities.keyboard_player);
        for flag in [
            "--screen-width",
            "--screen-height",
            "--present-mode",
            "--perf-overlay",
            "--fullscreen",
            "--controller-color",
            "--controller-volume",
            "--controller-vibration",
            "--keyboard-player",
        ] {
            assert!(capabilities.options.iter().any(|value| value == flag));
        }
    }
    #[test]
    fn configurable_cli_options_cover_the_upstream_linux_interface() {
        let expected = [
            "--addcont-dir",
            "--amd-cpu",
            "--command-buffer-dump",
            "--command-buffer-dump-folder",
            "--console-language",
            "--controller-color",
            "--controller-volume",
            "--controller-vibration",
            "--fullscreen",
            "--game-patch",
            "--gpu",
            "--gpu-assisted-validation",
            "--graphics-debug-dump",
            "--hdr",
            "--hide-cursor",
            "--keyboard-player",
            "--keymap",
            "--mic",
            "--network-connected",
            "--no-splash",
            "--perf-overlay",
            "--playgo-hack",
            "--present-mode",
            "--printf-direction",
            "--printf-output-file",
            "--profile",
            "--rd",
            "--readback-linear-images",
            "--screen-height",
            "--screen-width",
            "--shader-log-direction",
            "--shader-log-folder",
            "--shader-optimization-type",
            "--shader-validation",
            "--skip-notice-screen",
            "--spirv-debug-printf",
            "--sync-raw-image-buffers",
            "--tessellation",
            "--trophy-duration",
            "--no-trophy-notifications",
            "--user-id",
            "--user-name",
            "--vblank-frequency",
            "--vr",
            "--vulkan-validation",
        ];
        let mut actual = CONFIGURABLE_CLI_OPTIONS.to_vec();
        actual.sort_unstable();
        let mut expected = expected;
        expected.sort_unstable();
        assert_eq!(actual, expected);
        assert!(!CONFIGURABLE_CLI_OPTIONS.contains(&"--redzone"));
    }
    #[test]
    fn legacy_profile_settings_merge_into_inheritable_cli_options() {
        let mut profile = Profile {
            screen_width: Some(1600),
            screen_height: Some(900),
            perf_overlay: true,
            fullscreen: true,
            controller_volume: Some(65),
            ..Profile::default()
        };
        merge_legacy_profile_options(&mut profile);
        assert_eq!(profile.options["--screen-width"], "1600");
        assert_eq!(profile.options["--perf-overlay"], "true");
        assert_eq!(profile.options["--controller-volume"], "65");

        profile
            .options
            .insert("--screen-width".into(), "1920".into());
        profile
            .options
            .insert("--perf-overlay".into(), "false".into());
        sync_legacy_profile_fields(&mut profile);
        assert_eq!(profile.screen_width, Some(1920));
        assert!(!profile.perf_overlay);
    }
    #[test]
    fn manual_compatibility_outcomes_are_explicit_and_notes_are_bounded() {
        assert!(compatibility_outcome("reached_gameplay"));
        assert!(!compatibility_outcome("normal_exit"));
        assert_eq!(
            sanitize_compatibility_notes("first\nsecond\u{0}ok"),
            "first\nsecondok"
        );
        assert_eq!(sanitize_compatibility_notes(&"x".repeat(2500)).len(), 2000);
    }
    #[test]
    fn dynamic_profile_options_require_advertised_support_and_valid_values() {
        let capabilities = verified_profile_capabilities("  --hdr Enable HDR\n  --gpu-assisted-validation <bool> GPU validation\n  --vulkan-validation <bool> Vulkan validation\n  --console-language <0-29> language\n  --keymap <mapping> mapping\n");
        let mut options = BTreeMap::from([
            ("--hdr".into(), "true".into()),
            ("--console-language".into(), "3".into()),
            ("--keymap".into(), "Cross=K\nCircle=J".into()),
        ]);
        assert!(validate_extra_profile_options(&options, &capabilities).is_ok());
        options.insert("--console-language".into(), "30".into());
        assert!(validate_extra_profile_options(&options, &capabilities).is_err());
        options.remove("--console-language");
        options.insert("--totally-unknown".into(), "true".into());
        assert!(validate_extra_profile_options(&options, &capabilities).is_err());
        options.clear();
        options.insert("--gpu-assisted-validation".into(), "true".into());
        options.insert("--vulkan-validation".into(), "false".into());
        assert!(validate_extra_profile_options(&options, &capabilities).is_err());
    }
    #[test]
    fn future_fork_options_are_derived_from_help_and_reserved_modes_stay_hidden() {
        let capabilities = verified_profile_capabilities(
            "--future-mode <value> Future value\n--future-switch Enable future behavior\n--game <path> Game input\n--guest-exec <path> Guest execution mode\n--help Print help\n",
        );
        assert_eq!(
            capabilities.options,
            vec!["--future-mode", "--future-switch"]
        );
        assert_eq!(capabilities.switch_options, vec!["--future-switch"]);
        let options = BTreeMap::from([
            ("--future-mode".into(), "safe".into()),
            ("--future-switch".into(), "true".into()),
        ]);
        assert!(validate_extra_profile_options(&options, &capabilities).is_ok());
        let args = extra_profile_arguments(&options, &capabilities.switch_options)
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, vec!["--future-mode", "safe", "--future-switch"]);
        let forbidden = BTreeMap::from([("--guest-exec".into(), "/tmp/guest.elf".into())]);
        assert!(validate_extra_profile_options(&forbidden, &capabilities).is_err());
    }
    #[test]
    fn per_game_options_override_global_defaults_and_global_unsupported_values_are_skipped() {
        let capabilities =
            verified_profile_capabilities("--screen-width <num> Width\n--hdr Enable HDR\n");
        let global = BTreeMap::from([
            ("--screen-width".into(), "1600".into()),
            ("--old-fork-option".into(), "legacy".into()),
        ]);
        let game = BTreeMap::from([("--screen-width".into(), "1920".into())]);
        let effective = effective_profile_options(&global, &game, &capabilities).unwrap();
        assert_eq!(
            effective,
            BTreeMap::from([("--screen-width".into(), "1920".into())])
        );
        let unsupported_game = BTreeMap::from([("--old-fork-option".into(), "legacy".into())]);
        assert!(effective_profile_options(&global, &unsupported_game, &capabilities).is_err());
    }
    #[test]
    fn inherited_graphics_and_input_options_are_validated_and_passed_as_arguments() {
        let capabilities = verified_profile_capabilities(
            "--screen-width <num> Width\n--screen-height <num> Height\n--present-mode <value> Mode\n--perf-overlay <true|false> Overlay\n--fullscreen Fullscreen\n--controller-volume <0-100> Volume\n",
        );
        let options = BTreeMap::from([
            ("--screen-width".into(), "1920".into()),
            ("--screen-height".into(), "1080".into()),
            ("--present-mode".into(), "Mailbox".into()),
            ("--perf-overlay".into(), "false".into()),
            ("--fullscreen".into(), "true".into()),
            ("--controller-volume".into(), "80".into()),
        ]);
        assert!(validate_extra_profile_options(&options, &capabilities).is_ok());
        let args = extra_profile_arguments(&options, &capabilities.switch_options)
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--screen-width", "1920"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--screen-height", "1080"]));
        assert!(args.contains(&"--fullscreen".to_string()));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--perf-overlay", "false"]));
        let mut invalid = options;
        invalid.insert("--controller-volume".into(), "101".into());
        assert!(validate_extra_profile_options(&invalid, &capabilities).is_err());
    }
    #[test]
    fn guest_printf_output_can_be_a_new_file_but_user_ids_follow_upstream_limits() {
        let dir = std::env::temp_dir().join(format!("kyty-printf-{}", now()));
        fs::create_dir_all(&dir).unwrap();
        let caps = verified_profile_capabilities(
            "  --printf-output-file <path> Output\n  --user-id <num> User\n",
        );
        let options = BTreeMap::from([
            (
                "--printf-output-file".into(),
                dir.join("guest.log").display().to_string(),
            ),
            ("--user-id".into(), "253".into()),
        ]);
        assert!(validate_extra_profile_options(&options, &caps).is_ok());
        let max_user_id = i32::MAX - 3 * 0x100;
        let maximum = BTreeMap::from([("--user-id".into(), max_user_id.to_string())]);
        assert!(validate_extra_profile_options(&maximum, &caps).is_ok());
        assert!(
            canonicalize_option_path("--printf-output-file", &options["--printf-output-file"])
                .unwrap()
                .ends_with("guest.log")
        );
        let mut bad = options;
        bad.insert("--user-id".into(), "254".into());
        assert!(validate_extra_profile_options(&bad, &caps).is_err());
        let too_large = BTreeMap::from([("--user-id".into(), (max_user_id + 1).to_string())]);
        assert!(validate_extra_profile_options(&too_large, &caps).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn profile_arguments_keep_values_separate_and_repeat_keymaps() {
        let options = BTreeMap::from([
            ("--hdr".into(), "true".into()),
            ("--hide-cursor".into(), "false".into()),
            ("--gpu-assisted-validation".into(), "true".into()),
            ("--keymap".into(), "Cross=K\nCircle=J".into()),
        ]);
        let args = extra_profile_arguments(&options, &[])
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                "--vulkan-validation",
                "true",
                "--gpu-assisted-validation",
                "true",
                "--hdr",
                "--keymap",
                "Cross=K",
                "--keymap",
                "Circle=J"
            ]
        );
    }
    #[test]
    fn removing_a_game_hides_it_without_dropping_profile_or_session_history() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE games(id INTEGER PRIMARY KEY,hidden INTEGER NOT NULL DEFAULT 0); CREATE TABLE profiles(game_id INTEGER PRIMARY KEY,options_json TEXT NOT NULL); CREATE TABLE sessions(id INTEGER PRIMARY KEY,game_id INTEGER NOT NULL,build_id INTEGER,started_at INTEGER,ended_at INTEGER,log_path TEXT); INSERT INTO games VALUES(1,0); INSERT INTO profiles VALUES(1,'{\"--hdr\":\"true\"}'); INSERT INTO sessions VALUES(1,1,1,100,123,'/managed/log.log');").unwrap();
        soft_remove_game_entry(&mut db, 1).unwrap();
        let hidden: i64 = db
            .query_row("SELECT hidden FROM games WHERE id=1", [], |row| row.get(0))
            .unwrap();
        let profile: String = db
            .query_row(
                "SELECT options_json FROM profiles WHERE game_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let log: String = db
            .query_row("SELECT log_path FROM sessions WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(hidden, 1);
        assert_eq!(profile, "{\"--hdr\":\"true\"}");
        assert_eq!(log, "/managed/log.log");
        assert!(create_game_session(&mut db, 1, 1, 200, "/managed/new.log").is_err());

        db.execute("UPDATE games SET hidden=0 WHERE id=1", [])
            .unwrap();
        db.execute("UPDATE sessions SET ended_at=NULL WHERE id=1", [])
            .unwrap();
        assert!(soft_remove_game_entry(&mut db, 1).is_err());
        let hidden: i64 = db
            .query_row("SELECT hidden FROM games WHERE id=1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(hidden, 0);
    }
    #[test]
    fn managed_build_removal_preserves_history_and_protects_game_assignments() {
        let dir = std::env::temp_dir().join(format!("kyty-build-remove-{}", now()));
        let root = dir.join("builds");
        let install = root.join("commit-release");
        let replacement_install = root.join("replacement-release");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join(emulator_binary_name()), b"emulator").unwrap();
        fs::create_dir_all(&replacement_install).unwrap();
        fs::write(
            replacement_install.join(emulator_binary_name()),
            b"emulator",
        )
        .unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE builds(id INTEGER PRIMARY KEY,path TEXT,active INTEGER,validation_status TEXT,removed_at INTEGER,created_at INTEGER); CREATE TABLE games(id INTEGER PRIMARY KEY,build_id INTEGER); CREATE TABLE sessions(id INTEGER PRIMARY KEY,build_id INTEGER,ended_at INTEGER); CREATE TABLE community_build_results(id INTEGER PRIMARY KEY,build_id INTEGER); CREATE TABLE manual_compatibility_observations(id INTEGER PRIMARY KEY,build_id INTEGER); INSERT INTO builds VALUES(1,'placeholder',1,'verified',NULL,2); INSERT INTO builds VALUES(2,'replacement-placeholder',0,'verified',NULL,1);").unwrap();
        db.execute(
            "UPDATE builds SET path=? WHERE id=2",
            [replacement_install.display().to_string()],
        )
        .unwrap();
        db.execute(
            "UPDATE builds SET path=? WHERE id=1",
            [install.display().to_string()],
        )
        .unwrap();
        db.execute("INSERT INTO sessions VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(remove_managed_build(&mut db, 1, &root).is_err());
        db.execute("UPDATE sessions SET ended_at=123", []).unwrap();
        db.execute("INSERT INTO games VALUES(1,1)", []).unwrap();
        remove_managed_build(&mut db, 1, &root).unwrap();
        assert!(!install.exists());
        let active: i64 = db
            .query_row("SELECT id FROM builds WHERE active=1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(active, 2);
        let removed_at: Option<i64> = db
            .query_row("SELECT removed_at FROM builds WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(removed_at.is_some());
        let history_count: i64 = db
            .query_row(
                "SELECT count(*) FROM sessions WHERE build_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history_count, 1);
        let game_build: Option<i64> = db
            .query_row("SELECT build_id FROM games WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(game_build, None);
        #[cfg(unix)]
        {
            let outside = dir.join("unmanaged-install");
            fs::create_dir_all(&outside).unwrap();
            fs::write(outside.join("keep.txt"), b"keep").unwrap();
            let link = root.join("linked-build");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            db.execute(
            "INSERT INTO builds(id,path,active,validation_status,removed_at,created_at) VALUES(3,?,0,'verified',NULL,0)",
            [link.display().to_string()],
        )
        .unwrap();
            assert!(remove_managed_build(&mut db, 3, &root).is_err());
            db.execute(
                "UPDATE builds SET path=? WHERE id=3",
                [outside.display().to_string()],
            )
            .unwrap();
            assert!(remove_managed_build(&mut db, 3, &root).is_err());
            assert!(outside.join("keep.txt").is_file());
        }
        db.execute("INSERT INTO games VALUES(2,2)", []).unwrap();
        remove_managed_build(&mut db, 2, &root).unwrap();
        let active_count: i64 = db
            .query_row(
                "SELECT count(*) FROM builds WHERE active=1 AND removed_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let preferred_build: Option<i64> = db
            .query_row("SELECT build_id FROM games WHERE id=2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(active_count, 0);
        assert_eq!(preferred_build, None);
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn executable_validation_requires_a_successful_game_help_response() {
        let dir = std::env::temp_dir().join(format!("kyty-emulator-verify-{}", now()));
        fs::create_dir_all(&dir).unwrap();
        let executable = dir.join(emulator_binary_name());
        fs::write(
            &executable,
            "#!/bin/sh\nprintf 'usage: kyty_emulator --game <dir>\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify_emulator(&executable).is_ok());
        fs::write(
            &executable,
            "#!/bin/sh\ni=0\nwhile [ $i -lt 12000 ]; do printf 'usage output line %s\n' \"$i\"; i=$((i + 1)); done\nprintf '%s\\n' '--game <dir>'\n",
        )
        .unwrap();
        assert!(verify_emulator(&executable).is_ok());
        fs::write(
            &executable,
            "#!/bin/sh\ni=0\nwhile [ $i -lt 50000 ]; do printf 'usage output line %s\n' \"$i\"; i=$((i + 1)); done\nprintf '%s\\n' '--game <dir>'\n",
        )
        .unwrap();
        assert!(verify_emulator(&executable).is_err());
        fs::write(&executable, "#!/bin/sh\nprintf 'not an emulator'\n").unwrap();
        assert!(verify_emulator(&executable).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn parses_memory_values_from_proc_meminfo() {
        let sample = "MemTotal:       16384000 kB\nMemAvailable:    8192000 kB\n";
        assert_eq!(memory_value_bytes(sample, "MemTotal"), Some(16_777_216_000));
        assert_eq!(
            memory_value_bytes(sample, "MemAvailable"),
            Some(8_388_608_000)
        );
        assert_eq!(memory_value_bytes(sample, "SwapTotal"), None);
    }
    #[test]
    fn identifies_linux_controller_names_without_claiming_game_compatibility() {
        let input = "N: Name=\"Sony Interactive Entertainment DualSense Wireless Controller\"\nH: Handlers=event8 js0\n\nN: Name=\"Xbox Wireless Controller\"\nH: Handlers=event9 js1\n\nN: Name=\"USB Keyboard\"\nH: Handlers=event3 kbd\n\nN: Name=\"Generic USB Gamepad\"\nH: Handlers=event14\nB: KEY=0000000000000001\nB: ABS=0000000000000013\n\nN: Name=\"Generic Touchscreen\"\nH: Handlers=event15\nB: KEY=0000000000000001\nB: ABS=0000000000000013\n";
        let detected = detected_controllers(input);
        assert_eq!(detected.len(), 3);
        assert!(detected[0].starts_with("DualSense:"));
        assert!(detected[1].starts_with("Xbox:"));
        assert!(detected[2].starts_with("Generic controller: Generic USB Gamepad"));
    }
    #[test]
    fn repository_validation() {
        assert!(parse_repo(OFFICIAL).is_ok());
        assert!(parse_repo("https://github.com/x/y.git").is_ok());
        assert!(parse_repo("https://evil.com/x/y").is_err());
        assert!(parse_repo("https://user:token@github.com/x/y").is_err());
        assert!(parse_repo("https://github.com:444/x/y").is_err());
        assert!(parse_repo("https://github.com/x/y/extra").is_err());
        assert!(valid_branch_name("main"));
        assert!(valid_branch_name("release/1.2"));
        assert!(!valid_branch_name("--upload-pack=evil"));
        assert!(!valid_branch_name("bad..branch"));
    }
    #[test]
    fn reads_the_repository_default_checkout_branch_instead_of_assuming_main() {
        let dir = std::env::temp_dir().join(format!("kyty-branch-test-{}", now()));
        fs::create_dir_all(&dir).unwrap();
        let result = Command::new("git")
            .args(["init", "--quiet", "--initial-branch=release-line"])
            .current_dir(&dir)
            .status()
            .unwrap();
        assert!(result.success());
        assert_eq!(checked_out_branch(&dir).unwrap(), "release-line");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn update_intervals_are_limited_to_supported_settings() {
        for valid in ["0", "6", "24", "168"] {
            assert!(valid_update_interval(valid));
        }
        for invalid in ["-1", "1", "1e9", ""] {
            assert!(!valid_update_interval(invalid));
        }
    }
    #[test]
    fn release_asset_validation_accepts_encoded_installer_filenames_only_on_github() {
        let asset_url = "https://github.com/Febsho/KytyPs5-Studio/releases/download/v0.3.0/KytyPS5%20Studio_0.3.0_x64-setup.exe";
        let release = GithubAppRelease {
            tag_name: "v0.3.0".into(),
            html_url: "https://github.com/Febsho/KytyPs5-Studio/releases/tag/v0.3.0".into(),
            prerelease: false,
            draft: false,
            assets: Vec::new(),
        };
        let asset = GithubAppReleaseAsset {
            name: "KytyPS5 Studio_0.3.0_x64-setup.exe".into(),
            browser_download_url: asset_url.into(),
            size: 10,
            digest: None,
        };
        assert_eq!(release_asset_url(&release, &asset).unwrap(), asset_url);

        let untrusted = GithubAppReleaseAsset {
            browser_download_url: asset_url.replace("github.com", "example.com"),
            ..asset
        };
        assert!(release_asset_url(&release, &untrusted).is_err());
    }
    #[test]
    fn windows_update_selects_only_the_x64_nsis_installer() {
        let release = GithubAppRelease {
            tag_name: "v0.3.1".into(),
            html_url: "https://github.com/Febsho/KytyPs5-Studio/releases/tag/v0.3.1".into(),
            prerelease: false,
            draft: false,
            assets: [
                "KytyPS5-Studio-linux-x86_64.AppImage",
                "KytyPS5-Studio-windows-x64.exe",
                "KytyPS5-Studio-windows-x86-setup.exe",
                "KytyPS5-Studio-windows-x64-setup.exe",
            ]
            .into_iter()
            .map(|name| GithubAppReleaseAsset {
                name: name.into(),
                browser_download_url: String::new(),
                size: 1,
                digest: None,
            })
            .collect(),
        };

        assert_eq!(
            nsis_asset(&release).map(|asset| asset.name.as_str()),
            Some("KytyPS5-Studio-windows-x64-setup.exe")
        );
    }
    #[test]
    fn windows_update_arguments_preserve_unicode_custom_install_paths() {
        let executable = std::env::temp_dir()
            .join("KytyPS5 Studio")
            .join("保存したゲーム")
            .join("kytyps5-studio.exe");
        let install_directory = executable.parent().unwrap();
        let mut expected_install_argument = OsString::from("/D=");
        expected_install_argument.push(install_directory.as_os_str());

        let arguments = nsis_update_arguments(&executable).unwrap();

        assert_eq!(arguments[0], "/UPDATE");
        assert_eq!(arguments[1], "/P");
        assert_eq!(arguments[2], "/R");
        assert_eq!(arguments[3], expected_install_argument);
        assert!(arguments[3].to_string_lossy().contains("保存したゲーム"));
    }
    #[cfg(windows)]
    #[test]
    fn explorer_path_is_resolved_from_system_root_not_path_search() {
        assert_eq!(
            super::windows_explorer_path(std::ffi::OsStr::new(r"C:\Windows")),
            PathBuf::from(r"C:\Windows\explorer.exe")
        );
    }
    #[test]
    fn update_preflight_blocks_only_sessions_that_have_not_ended() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE sessions(ended_at INTEGER);
                 INSERT INTO sessions VALUES(NULL);
                 INSERT INTO sessions VALUES(123);",
            )
            .unwrap();
        assert!(has_active_game_sessions(&connection).unwrap());

        connection
            .execute(
                "UPDATE sessions SET ended_at=456 WHERE ended_at IS NULL",
                [],
            )
            .unwrap();
        assert!(!has_active_game_sessions(&connection).unwrap());
    }
    #[test]
    fn automatic_update_policy_is_limited_to_safe_modes() {
        assert!(valid_auto_update_mode("notify"));
        assert!(valid_auto_update_mode("build"));
        assert!(!valid_auto_update_mode("activate"));
        assert!(!valid_auto_update_mode("delete-old-builds"));
    }
    #[test]
    fn webkit_dmabuf_workaround_is_limited_to_nvidia_wayland_without_user_override() {
        let nvidia = vec!["0x10de".to_string()];
        assert!(should_disable_dmabuf_renderer("wayland", false, &nvidia));
        assert!(!should_disable_dmabuf_renderer("x11", false, &nvidia));
        assert!(!should_disable_dmabuf_renderer("wayland", true, &nvidia));
        assert!(!should_disable_dmabuf_renderer(
            "wayland",
            false,
            &["0x1002".into()]
        ));
        assert!(!should_disable_dmabuf_renderer("wayland", false, &[]));
    }
    #[test]
    fn update_status_compares_built_commit_as_well_as_source() {
        assert_eq!(
            update_status("abc123", Some("abc123"), "abc123"),
            "Up to date"
        );
        assert_eq!(
            update_status("def456", Some("abc123"), "def456"),
            "Update available: abc123 → def456; source is already at this commit"
        );
        assert!(update_status("abc123", None, "abc123")
            .starts_with("Update available: no verified build → abc123"));
        assert!(update_status("def456", Some("abc123"), "abc123")
            .contains("Installed build is current"));
    }
    #[test]
    fn build_options_are_validated_before_starting_jobs() {
        let options = BuildOptions {
            build_type: "Release".into(),
            jobs: 4,
            compiler: None,
            clean: false,
        };
        assert_eq!(
            validate_build_options(&options).unwrap(),
            default_build_compiler()
        );
        let invalid = BuildOptions {
            build_type: "Other".into(),
            ..options
        };
        assert!(validate_build_options(&invalid).is_err());
    }
    #[test]
    fn compiler_selection_tracks_the_host_abi_without_changing_linux() {
        assert_eq!(default_build_compiler_for(true), "clang-cl");
        assert_eq!(default_build_compiler_for(false), "clang");
    }

    #[test]
    fn jobs_receive_distinct_ids_and_reject_overlapping_starts() {
        let jobs = Jobs {
            pid: AtomicI32::new(0),
            active: AtomicBool::new(false),
            updating: AtomicBool::new(false),
            job_id: AtomicU64::new(0),
            event_kind: Mutex::new(String::new()),
            cancel_requested: AtomicBool::new(false),
            cancellable: AtomicBool::new(false),
            control: Mutex::new(()),
        };
        jobs.updating.store(true, Ordering::SeqCst);
        assert!(begin_job(&jobs, "Already running", true, "build").is_err());
        jobs.updating.store(false, Ordering::SeqCst);
        let first = begin_job(&jobs, "Already running", true, "build").unwrap();
        assert_eq!(jobs.job_id.load(Ordering::SeqCst), first);
        assert_eq!(ACTIVE_JOB_ID.load(Ordering::SeqCst), first);
        assert!(begin_job(&jobs, "Already running", true, "clone").is_err());

        jobs.active.store(false, Ordering::SeqCst);
        ACTIVE_JOB_ID.store(0, Ordering::SeqCst);
        let second = begin_job(&jobs, "Already running", true, "clone").unwrap();
        assert_ne!(first, second);
        ACTIVE_JOB_ID.store(0, Ordering::SeqCst);
    }

    #[test]
    fn update_reservation_releases_after_failure_and_persists_after_installer_start() {
        let jobs = Arc::new(Jobs {
            pid: AtomicI32::new(0),
            active: AtomicBool::new(false),
            updating: AtomicBool::new(true),
            job_id: AtomicU64::new(0),
            event_kind: Mutex::new(String::new()),
            cancel_requested: AtomicBool::new(false),
            cancellable: AtomicBool::new(false),
            control: Mutex::new(()),
        });
        {
            let _reservation = UpdateReservation {
                jobs: jobs.clone(),
                preserve: false,
            };
        }
        assert!(!jobs.updating.load(Ordering::SeqCst));

        jobs.updating.store(true, Ordering::SeqCst);
        {
            let mut reservation = UpdateReservation {
                jobs: jobs.clone(),
                preserve: false,
            };
            reservation.preserve = true;
        }
        assert!(jobs.updating.load(Ordering::SeqCst));
    }

    #[test]
    fn structured_job_channels_separate_repository_runtime_and_build_output() {
        assert_eq!(job_event_channel("clone", "Cloning"), "repository-clone");
        assert_eq!(job_event_channel("build", "Fetching"), "repository-update");
        assert_eq!(job_event_channel("build", "Compile"), "emulator-build");
        assert_eq!(
            job_event_channel("community-build", "Compile"),
            "community-pr-build"
        );
        assert_eq!(job_event_channel("game", "Running"), "runtime");
    }
    #[test]
    fn standard_build_checkout_validation_rejects_dirty_and_uninitialized_sources() {
        let root = env::temp_dir().join(format!(
            "kyty-clean-source-test-{}-{}",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&root).unwrap();
        test_git(&root, &["init", "--quiet"]).unwrap();
        test_git(&root, &["config", "user.name", "Test"]).unwrap();
        test_git(&root, &["config", "user.email", "test@localhost"]).unwrap();
        fs::write(root.join("source.txt"), "clean\n").unwrap();
        test_git(&root, &["add", "."]).unwrap();
        test_git(
            &root,
            &["-c", "commit.gpgsign=false", "commit", "-m", "base"],
        )
        .unwrap();
        assert!(validate_clean_source_checkout(&root).is_ok());
        fs::write(root.join("source.txt"), "modified\n").unwrap();
        assert!(validate_clean_source_checkout(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn standard_build_checkout_validation_rejects_dirty_recursive_submodules() {
        let root = env::temp_dir().join(format!(
            "kyty-dirty-submodule-test-{}-{}",
            std::process::id(),
            now()
        ));
        let nested = root.join("nested");
        let source = root.join("source");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&source).unwrap();
        test_git(&nested, &["init", "--quiet", "-b", "main"]).unwrap();
        test_git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
        for path in [&nested, &source] {
            test_git(path, &["config", "user.name", "Test"]).unwrap();
            test_git(path, &["config", "user.email", "test@localhost"]).unwrap();
        }
        fs::write(nested.join("nested.txt"), "clean\n").unwrap();
        test_git(&nested, &["add", "."]).unwrap();
        test_git(
            &nested,
            &["-c", "commit.gpgsign=false", "commit", "-m", "nested"],
        )
        .unwrap();
        fs::write(source.join("root.txt"), "clean\n").unwrap();
        test_git(&source, &["add", "."]).unwrap();
        test_git(
            &source,
            &["-c", "commit.gpgsign=false", "commit", "-m", "base"],
        )
        .unwrap();
        // Keep the filesystem URL free of the `\\?\` verbatim prefix that
        // `canonicalize` adds on Windows; Git parses that prefix as a URI
        // authority instead of a local path.
        let nested_url = nested.display().to_string();
        test_git(
            &source,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &nested_url,
                "vendor/upstream",
            ],
        )
        .unwrap();
        test_git(&source, &["add", ".gitmodules", "vendor/upstream"]).unwrap();
        test_git(
            &source,
            &["-c", "commit.gpgsign=false", "commit", "-m", "submodule"],
        )
        .unwrap();
        assert!(validate_clean_source_checkout(&source).is_ok());
        test_git(
            &source,
            &["submodule", "deinit", "--force", "vendor/upstream"],
        )
        .unwrap();
        assert!(validate_clean_source_checkout(&source)
            .unwrap_err()
            .contains("submodules are uninitialized"));
        assert!(ensure_submodules_initialized(&source)
            .unwrap_err()
            .contains("Checkout is incomplete"));
        assert!(validate_source_before_submodule_update(&source).is_ok());
        test_git(
            &source,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
                "--recursive",
            ],
        )
        .unwrap();
        assert!(ensure_submodules_initialized(&source).is_ok());
        assert!(validate_clean_source_checkout(&source).is_ok());
        fs::write(source.join("vendor/upstream/nested.txt"), "dirty\n").unwrap();
        let error = validate_clean_source_checkout(&source).unwrap_err();
        assert!(error.contains("submodules contain modified or untracked files"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn build_dependency_preflight_reports_only_missing_tools() {
        assert!(missing_build_dependencies(|_| true).is_empty());
        assert!(any_command_available(
            &["glslangValidator", "glslang"],
            |name| name == "glslang"
        ));
        assert_eq!(
            missing_build_dependencies(|name| name != "git"),
            vec!["git"]
        );
        assert_eq!(
            missing_build_dependencies(|name| name != "ninja" && name != "python3"),
            vec!["ninja", "python3"]
        );
        assert_eq!(
            missing_pkg_config_dependencies(|name| name != "libudev"),
            vec!["libudev"]
        );
    }
    #[test]
    fn linux_x86_64_emulator_build_enables_sse41_without_changing_other_targets() {
        assert_eq!(
            linux_x86_64_cmake_cpu_flags("linux", "x86_64"),
            vec!["-DCMAKE_CXX_FLAGS=-msse4.1"]
        );
        assert!(linux_x86_64_cmake_cpu_flags("windows", "x86_64").is_empty());
        assert!(linux_x86_64_cmake_cpu_flags("linux", "aarch64").is_empty());
    }
    #[cfg(not(windows))]
    #[test]
    fn linux_clang_intrinsics_workaround_is_scoped_to_affected_x86_checkout() {
        let root = env::temp_dir().join(format!("kyty-intrin-compat-{} space", now()));
        let source = root.join("source");
        let build = root.join("build");
        let affected = source.join("src/graphics/host_gpu/renderer/cache/bufferCache.cpp");
        fs::create_dir_all(affected.parent().unwrap()).unwrap();
        fs::write(&affected, "#include <intrin.h>\n").unwrap();

        let flags = linux_x86_64_intrin_compat_flags("linux", "x86_64", &source, &build).unwrap();
        assert_eq!(flags.len(), 1);
        assert!(flags[0].contains("-I\""));
        assert_eq!(
            fs::read_to_string(build.join("compat/include/intrin.h")).unwrap(),
            "#include <x86intrin.h>\n"
        );
        assert!(
            linux_x86_64_intrin_compat_flags("windows", "x86_64", &source, &build)
                .unwrap()
                .is_empty()
        );
        fs::write(&affected, "#include <x86intrin.h>\n").unwrap();
        assert!(
            linux_x86_64_intrin_compat_flags("linux", "x86_64", &source, &build)
                .unwrap()
                .is_empty()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn parses_proc_stat_fields_with_parenthesized_process_names() {
        let stat =
            "123 (game process (worker)) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 2048 21";
        let parsed = parse_proc_stat(stat, 4096).unwrap();
        assert_eq!(parsed.state, 'S');
        assert_eq!(parsed.cpu_ticks, 23);
        assert_eq!(parsed.start_ticks, 19);
        assert_eq!(parsed.resident_bytes, 21 * 4096);

        let self_stat = fs::read_to_string(format!("/proc/{}/stat", std::process::id())).unwrap();
        assert!(parse_proc_stat(&self_stat, 4096).is_some());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn matches_a_process_by_its_registered_executable_when_start_ticks_are_missing() {
        let executable = std::env::current_exe().unwrap();
        assert!(process_executable_matches(std::process::id(), &executable));
        assert!(!process_executable_matches(
            std::process::id(),
            Path::new("/bin/false")
        ));
    }
    #[test]
    fn stopping_requires_a_recorded_process_incarnation_and_matching_group() {
        let actual = ProcStat {
            state: 'S',
            process_group: 42,
            cpu_ticks: 0,
            start_ticks: 9876,
            resident_bytes: 0,
        };
        assert!(verified_session_process(42, Some(9876), actual, true));
        assert!(!verified_session_process(42, None, actual, true));
        assert!(!verified_session_process(42, Some(9875), actual, true));
        assert!(!verified_session_process(41, Some(9876), actual, true));
        assert!(!verified_session_process(42, Some(9876), actual, false));
    }
    #[test]
    fn session_identity_accepts_windows_pids_above_signed_32_bit_range() {
        let pid = i64::from(i32::MAX as u32 + 1);
        let actual = ProcStat {
            state: 'R',
            process_group: pid,
            cpu_ticks: 0,
            start_ticks: 9876,
            resident_bytes: 0,
        };
        assert!(verified_session_process(pid, Some(9876), actual, true));
    }
    #[test]
    fn build_registration_reports_ignored_installation_paths() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE builds(repo_id INTEGER,commit_sha TEXT,build_type TEXT,path TEXT UNIQUE,created_at INTEGER,install_size_bytes INTEGER,validation_status TEXT,configuration_json TEXT);",
            )
            .unwrap();
        let path = Path::new("/managed/builds/first");
        assert!(insert_registered_build(
            &connection,
            1,
            "abc123",
            "Release",
            path,
            1,
            Some(1024),
            "{}",
        )
        .unwrap());
        assert!(!insert_registered_build(
            &connection,
            1,
            "def456",
            "Release",
            path,
            2,
            Some(2048),
            "{}",
        )
        .unwrap());
        let count: i64 = connection
            .query_row("SELECT count(*) FROM builds", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn calculates_cpu_percent_from_tick_deltas_and_elapsed_time() {
        assert_eq!(
            cpu_percent(100, 150, Duration::from_secs(1), 100.0),
            Some(50.0)
        );
        assert_eq!(
            cpu_percent(100, 100, Duration::from_secs(2), 100.0),
            Some(0.0)
        );
        assert_eq!(cpu_percent(100, 150, Duration::ZERO, 100.0), None);
    }
    #[test]
    fn adapter_detection() {
        let dir = std::env::temp_dir().join(format!("kyty-adapter-test-{}", now()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("CMakeLists.txt"),
            "add_executable(kyty_emulator x) KYTY_BUILD_LAUNCHER",
        )
        .unwrap();
        std::fs::write(dir.join("src/main.cpp"), "--game").unwrap();
        assert_eq!(inspect_adapter(&dir), "kyty-cmake");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn game_paths_require_a_supported_file_or_directory_layout() {
        let dir = std::env::temp_dir().join(format!("kyty-game-path-test-{}", now()));
        fs::create_dir_all(dir.join("folder/sce_sys")).unwrap();
        fs::write(dir.join("folder/eboot.bin"), b"\x7fELF\x02\x01").unwrap();
        fs::create_dir_all(dir.join("fake_folder")).unwrap();
        fs::write(dir.join("fake_folder/eboot.bin"), b"not an executable").unwrap();
        write_zarchive_fixture(&dir.join("game.ZAR"));
        fs::write(dir.join("broken.zar"), b"not an archive").unwrap();
        fs::write(dir.join("game.elf"), b"\x7fELF\x02\x01").unwrap();
        fs::write(dir.join("fake.elf"), b"not an executable").unwrap();
        fs::write(dir.join("notes.txt"), b"no").unwrap();
        assert!(supported_game_path(&dir.join("folder")));
        assert!(!supported_game_path(&dir.join("fake_folder")));
        assert!(!supported_game_path(&dir.join("fake_folder/eboot.bin")));
        assert!(supported_game_path(&dir.join("game.ZAR")));
        assert!(!supported_game_path(&dir.join("broken.zar")));
        assert!(supported_game_path(&dir.join("game.elf")));
        assert!(!supported_game_path(&dir.join("fake.elf")));
        assert!(!supported_game_path(&dir.join("notes.txt")));
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn windows_game_import_accepts_case_variant_eboot_filename() {
        let dir = env::temp_dir().join(format!("kyty-windows-eboot-case-{}", now()));
        let game = dir.join("Title");
        fs::create_dir_all(&game).unwrap();
        let eboot = game.join("EBOOT.BIN");
        fs::write(&eboot, b"\x7fELF\x02\x01").unwrap();

        assert!(supported_game_path(&game));
        assert!(supported_game_path(&eboot));
        assert_eq!(game_artwork_root(&eboot), Some(game));

        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recursive_scan_does_not_import_content_nested_under_an_eboot_game() {
        let dir = std::env::temp_dir().join(format!("kyty-game-scan-ancestor-{}", now()));
        let game = dir.join("collection/title");
        fs::create_dir_all(game.join("sce_sys")).unwrap();
        fs::write(game.join("eboot.bin"), b"\x7fELF\x02\x01").unwrap();
        fs::write(game.join("bonus.elf"), b"\x7fELF\x02\x01").unwrap();
        fs::write(game.join("extra.zar"), b"\x7fZAR\0\0\0\0").unwrap();
        let nested = game.join("sce_sys/param.sfo");
        let mut eboot_cache = HashMap::new();
        assert!(has_game_ancestor_cached(
            &nested,
            &dir,
            &mut eboot_cache,
            false,
        ));
        assert!(has_game_ancestor_cached(
            &game.join("bonus.elf"),
            &dir,
            &mut eboot_cache,
            false,
        ));
        assert!(!has_game_ancestor_cached(
            &dir.join("standalone.elf"),
            &dir,
            &mut eboot_cache,
            false,
        ));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn windows_scan_treats_case_variant_eboot_as_a_game_ancestor() {
        let dir = env::temp_dir().join(format!(
            "kyty-windows-game-scan-case-{}-{}",
            std::process::id(),
            now()
        ));
        let game = dir.join("collection/title");
        fs::create_dir_all(game.join("sce_sys")).unwrap();
        fs::write(game.join("EBOOT.BIN"), b"\x7fELF\x02\x01").unwrap();
        fs::write(game.join("bonus.elf"), b"\x7fELF\x02\x01").unwrap();

        assert!(directory_contains_game_eboot(&game, true));
        assert!(has_game_ancestor_cached(
            &game.join("bonus.elf"),
            &dir,
            &mut HashMap::new(),
            true,
        ));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn title_id_evidence_is_read_only_from_small_managed_logs() {
        let dir = std::env::temp_dir().join(format!("kyty-log-evidence-{}", now()));
        let logs = dir.join("logs");
        fs::create_dir_all(&logs).unwrap();
        let managed = logs.join("session.log");
        let external = dir.join("unrelated.log");
        fs::write(&managed, "Title ID: PPSA00001\n").unwrap();
        fs::write(&external, "Title ID: PPSA00002\n").unwrap();
        assert!(log_reports_title_id(&logs, &managed));
        assert!(!log_reports_title_id(&logs, &external));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn log_tail_reader_caps_large_files_and_marks_truncation() {
        let dir = std::env::temp_dir().join(format!("kyty-log-tail-{}", now()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("large.log");
        fs::write(&path, "0123456789").unwrap();
        let tail = read_bounded_tail(&path, 4).unwrap();
        assert!(tail.starts_with("[Earlier log content omitted; showing last 4 bytes]"));
        assert!(tail.ends_with("6789"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn log_export_copies_atomically_and_enforces_the_size_limit() {
        let dir = std::env::temp_dir().join(format!("kyty-log-export-{}", now()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("session.log");
        let destination = dir.join("exported.log");
        fs::write(&source, "first session").unwrap();
        fs::write(&destination, "older file").unwrap();
        copy_file_atomically(&source, &destination, 64).unwrap();
        assert_eq!(fs::read_to_string(&destination).unwrap(), "first session");
        assert!(copy_file_atomically(&source, &destination, 4).is_err());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "first session");
        fs::remove_dir_all(dir).unwrap();
    }
}
