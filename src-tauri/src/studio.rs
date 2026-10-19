use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    os::unix::fs::PermissionsExt,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};

#[cfg(test)]
const OFFICIAL: &str = "https://github.com/KytyPS5/KytyPS5";
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is unavailable".into())
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
        let path = |key: &str, fallback: &str| {
            env::var_os(key)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| h.join(fallback))
                .join("kytyps5-studio")
        };
        let p = Self {
            data: path("XDG_DATA_HOME", ".local/share"),
            cache: path("XDG_CACHE_HOME", ".cache"),
            config: path("XDG_CONFIG_HOME", ".config"),
        };
        for d in [&p.data, &p.cache, &p.config] {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        Ok(p)
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
    last_session_at: Option<i64>,
    last_session_outcome: Option<String>,
    last_session_exit_code: Option<i32>,
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
}

fn profile_capabilities(help: &str) -> ProfileCapabilities {
    let has = |flag: &str| help.lines().any(|line| line.trim_start().starts_with(flag));
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

fn capabilities_for_build(b: &Build) -> Result<ProfileCapabilities, String> {
    let exe = Path::new(&b.path).join("kyty_emulator");
    let help = emulator_help(&exe)?;
    Ok(profile_capabilities(&help))
}
fn profile(c: &Connection, game_id: i64) -> Result<Profile, String> {
    Ok(c.query_row(
        "SELECT screen_width,screen_height,present_mode,perf_overlay,fullscreen,controller_color,controller_volume,controller_vibration,keyboard_player FROM profiles WHERE game_id=?",
        [game_id],
        |r| {
            Ok(Profile {
                screen_width: r.get(0)?,
                screen_height: r.get(1)?,
                present_mode: r.get(2)?,
                perf_overlay: r.get::<_, i64>(3)? != 0,
                fullscreen: r.get::<_, i64>(4)? != 0,
                controller_color: r.get(5)?, controller_volume: r.get(6)?, controller_vibration: r.get(7)?, keyboard_player: r.get(8)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .unwrap_or_default())
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
#[derive(Serialize, Clone)]
struct JobEvent {
    kind: String,
    phase: String,
    line: String,
    done: bool,
    success: bool,
}
struct Jobs {
    pid: AtomicI32,
    active: AtomicBool,
    cancel_requested: AtomicBool,
    cancellable: AtomicBool,
    control: Mutex<()>,
}
fn begin_job(jobs: &Jobs, message: &str, cancellable: bool) -> Result<(), String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if jobs.active.swap(true, Ordering::SeqCst) {
        return Err(message.to_string());
    }
    jobs.cancel_requested.store(false, Ordering::SeqCst);
    jobs.cancellable.store(cancellable, Ordering::SeqCst);
    jobs.pid.store(0, Ordering::SeqCst);
    Ok(())
}
fn terminate_process_group(pid: i32) -> std::io::Result<()> {
    let result = unsafe { libc::kill(-pid, libc::SIGTERM) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[derive(Default)]
struct ProcessSamples(Mutex<HashMap<i64, ProcessSample>>);
#[derive(Default)]
struct ProcessSample {
    previous: Option<(u64, Instant)>,
    missing_samples: u8,
}
#[derive(Clone, Copy, Debug)]
struct ProcStat {
    state: char,
    cpu_ticks: u64,
    start_ticks: u64,
    resident_bytes: u64,
}
fn parse_proc_stat(contents: &str, page_size: u64) -> Option<ProcStat> {
    let close = contents.rfind(')')?;
    let fields: Vec<_> = contents.get(close + 1..)?.split_whitespace().collect();
    let state = fields.first()?.chars().next()?;
    let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
    let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
    let start_ticks = fields.get(19)?.parse::<u64>().ok()?;
    let resident_pages = fields.get(21)?.parse::<i64>().ok()?.max(0) as u64;
    Some(ProcStat {
        state,
        cpu_ticks: user_ticks.saturating_add(system_ticks),
        start_ticks,
        resident_bytes: resident_pages.saturating_mul(page_size),
    })
}
fn proc_stat(pid: u32) -> Option<ProcStat> {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return None;
    }
    let contents = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_proc_stat(&contents, page_size as u64)
}
fn process_executable_matches(pid: u32, expected_executable: &Path) -> bool {
    fs::canonicalize(format!("/proc/{pid}/exe"))
        .ok()
        .zip(fs::canonicalize(expected_executable).ok())
        .is_some_and(|(actual, expected)| actual == expected)
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
fn db(app: &tauri::AppHandle) -> Result<Connection, String> {
    state_paths(app)?.db()
}
fn command_exists(name: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|p| p.join(name).is_file()))
        .unwrap_or(false)
}
fn graphics_diagnostics() -> GraphicsDiagnostics {
    let mut drivers = Vec::new();
    let mut monitors = Vec::new();
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
    drivers.sort();
    drivers.dedup();
    monitors.sort();

    let (vulkan_status, vulkan_details) = if !command_exists("vulkaninfo") {
        (
            "Unavailable".into(),
            "Install vulkan-tools and a Vulkan ICD/driver package, then retry diagnostics.".into(),
        )
    } else {
        match Command::new("vulkaninfo")
            .arg("--summary")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
        {
            Err(error) => (
                "Failed".into(),
                format!("Could not start vulkaninfo: {error}"),
            ),
            Ok(mut child) => {
                let deadline = Instant::now() + Duration::from_secs(6);
                let mut poll_error = None;
                let status = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break Some(status),
                        Ok(None) if Instant::now() < deadline => {
                            thread::sleep(Duration::from_millis(30));
                        }
                        Ok(None) => {
                            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
                            let _ = child.wait();
                            break None;
                        }
                        Err(error) => {
                            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
                            let _ = child.wait();
                            poll_error = Some(error.to_string());
                            break None;
                        }
                    }
                };
                if let Some(error) = poll_error {
                    (
                        "Failed".into(),
                        format!("Could not check vulkaninfo status: {error}"),
                    )
                } else if status.is_none() {
                    (
                        "Timed out".into(),
                        "vulkaninfo did not finish within six seconds.".into(),
                    )
                } else {
                    match child.wait_with_output() {
                        Err(error) => (
                            "Failed".into(),
                            format!("Could not read vulkaninfo output: {error}"),
                        ),
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
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
                                .take(20)
                                .collect::<Vec<_>>()
                                .join("\n");
                            if !output.status.success() {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                (
                                    "Failed".into(),
                                    format!(
                                        "{}\n{}",
                                        lines,
                                        stderr.lines().take(8).collect::<Vec<_>>().join("\n")
                                    ),
                                )
                            } else if !stdout.contains("GPU") && !stdout.contains("deviceName") {
                                ("No device found".into(), "vulkaninfo ran, but no Vulkan physical device was reported. Check the installed Vulkan ICD and active GPU driver.".into())
                            } else {
                                ("Available".into(), lines)
                            }
                        }
                    }
                }
            }
        }
    };
    GraphicsDiagnostics {
        vulkan_status,
        vulkan_details,
        drivers,
        monitors,
    }
}
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
fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new("git");
    if let Some(d) = dir {
        c.arg("-C").arg(d);
    }
    let o = c.args(args).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
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
    let mut command = Command::new("git");
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
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let pid = child.id() as i32;
    jobs.pid.store(pid, Ordering::SeqCst);
    let stdout = child.stdout.take().ok_or("Git stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("Git stderr unavailable")?;
    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = BufReader::new(stdout).read_to_end(&mut output);
        output
    });
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = BufReader::new(stderr).read_to_end(&mut output);
        output
    });
    let status = loop {
        if jobs.cancel_requested.load(Ordering::SeqCst) {
            let _ = terminate_process_group(pid);
        }
        if let Some(exit) = child.try_wait().map_err(|error| error.to_string())? {
            break exit;
        }
        thread::sleep(Duration::from_millis(100));
    };
    jobs.pid.store(0, Ordering::SeqCst);
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Git output reader failed")?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Git error reader failed")?;
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
    if !stdout.is_empty() {
        emit(app, "build", "Git", stdout.clone(), false, true);
    }
    if !stderr.is_empty() {
        emit(app, "build", "Git", stderr, false, true);
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
    !branch.is_empty()
        && Command::new("git")
            .args(["check-ref-format", "--branch", branch])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
}
fn repo(conn: &Connection, id: i64) -> Result<Repo, String> {
    conn.query_row("SELECT id,owner,name,url,branch,source_path,adapter,installed_commit FROM repositories WHERE id=?",[id],|r|Ok(Repo{id:r.get(0)?,owner:r.get(1)?,name:r.get(2)?,url:r.get(3)?,branch:r.get(4)?,source_path:r.get(5)?,adapter:r.get(6)?,installed_commit:r.get(7)?})).map_err(|e|e.to_string())
}
fn open_directory(path: &Path) -> Result<(), String> {
    let directory =
        fs::canonicalize(path).map_err(|error| format!("Folder is unavailable: {error}"))?;
    if !directory.is_dir() {
        return Err("Selected location is not a directory".into());
    }
    let mut child = Command::new("xdg-open")
        .arg(directory)
        .spawn()
        .map_err(|error| {
            format!("Could not open the folder with the desktop file manager: {error}")
        })?;
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
fn build(conn: &Connection, id: i64) -> Result<Build, String> {
    conn.query_row(
        "SELECT id,repo_id,commit_sha,build_type,path,created_at,active,install_size_bytes,validation_status,
         (SELECT started_at FROM sessions WHERE build_id=builds.id ORDER BY started_at DESC,id DESC LIMIT 1),
         (SELECT outcome FROM sessions WHERE build_id=builds.id ORDER BY started_at DESC,id DESC LIMIT 1),
         (SELECT exit_code FROM sessions WHERE build_id=builds.id ORDER BY started_at DESC,id DESC LIMIT 1)
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
                    let executable = Path::new(&r.get::<_, String>(4)?).join("kyty_emulator");
                    match fs::metadata(&executable) {
                        Ok(metadata) if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 => "Executable present".to_string(),
                        Ok(metadata) if metadata.is_file() => "Not executable".to_string(),
                        _ => "Missing".to_string(),
                    }
                },
                install_size_bytes: r.get::<_, Option<i64>>(7)?.and_then(|size| u64::try_from(size).ok()),
                validation_status: r.get(8)?,
                last_session_at: r.get(9)?,
                last_session_outcome: r.get(10)?,
                last_session_exit_code: r.get(11)?,
            })
        },
    )
    .map_err(|e| e.to_string())
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
            })
        },
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
fn list_repos(app: tauri::AppHandle) -> Result<Vec<Repo>, String> {
    let c = db(&app)?;
    let mut s = c
        .prepare("SELECT id FROM repositories ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
    let result = rows
        .map(|id| repo(&c, id.map_err(|e| e.to_string())?))
        .collect();
    result
}
#[tauri::command]
fn list_builds(app: tauri::AppHandle) -> Result<Vec<Build>, String> {
    let c = db(&app)?;
    let mut s = c
        .prepare("SELECT id FROM builds ORDER BY created_at DESC,id DESC")
        .map_err(|e| e.to_string())?;
    let rows = s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
    let result = rows
        .map(|id| build(&c, id.map_err(|e| e.to_string())?))
        .collect();
    result
}
#[tauri::command]
fn list_games(app: tauri::AppHandle) -> Result<Vec<Game>, String> {
    let c = db(&app)?;
    let mut s = c
        .prepare("SELECT id FROM games ORDER BY favorite DESC, title COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
    let result = rows
        .map(|id| game(&c, id.map_err(|e| e.to_string())?))
        .collect();
    result
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
fn log_reports_title_id(log_root: &Path, log_path: &Path) -> bool {
    let Ok(root) = fs::canonicalize(log_root) else {
        return false;
    };
    let Ok(path) = fs::canonicalize(log_path) else {
        return false;
    };
    if !path.starts_with(root) {
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
        "INSERT OR IGNORE INTO repositories(owner,name,url,branch) VALUES(?,?,?,?)",
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
fn github_token() -> Option<String> {
    for variable in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(token) = env::var(variable) {
            if let Ok(token) = validate_github_token(&token) {
                return Some(token);
            }
        }
    }
    let output = Command::new("secret-tool")
        .args(["lookup", "service", "kytyps5-studio", "account", "github"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?;
    validate_github_token(token.trim_end_matches(['\r', '\n'])).ok()
}
#[tauri::command]
fn save_github_token(token: String) -> Result<(), String> {
    let token = validate_github_token(&token)?;
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
#[tauri::command]
fn delete_github_token() -> Result<(), String> {
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
#[tauri::command]
fn github_token_status() -> bool {
    github_token().is_some()
}
fn github_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let mut request = ureq::get(url)
        .header("User-Agent", "KytyPS5-Studio")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = github_token() {
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
#[tauri::command]
fn list_community_pull_requests(
    app: tauri::AppHandle,
    repository_id: i64,
    page: u32,
    state: String,
) -> Result<CommunityPrPage, String> {
    if page == 0 || page > 1000 {
        return Err("Pull request page must be between 1 and 1000".into());
    }
    if !["open", "closed", "all"].contains(&state.as_str()) {
        return Err("Pull request state must be open, closed, or all".into());
    }
    let conn = db(&app)?;
    let repository = repo(&conn, repository_id)?;
    if repository.source_path.is_none() {
        return Err("Clone the selected repository before browsing its pull requests".into());
    }
    let (owner, name, _) = parse_repo(&repository.url)?;
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
        .take(100)
        .collect::<Vec<_>>();
    let has_more = start.saturating_add(items.len()) < total_cached;
    Ok(CommunityPrPage {
        repository: format!("{owner}/{name}"),
        items,
        total_count: None,
        page,
        per_page: 100,
        has_more,
        stale: true,
        refresh_error: Some(error),
    })
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
) -> Result<(), String> {
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
    if !source.is_dir() || !git(Some(&source), &["status", "--porcelain"])?.is_empty() {
        return Err("The tracked source checkout is missing or has local changes; community builds use a clean isolated clone".into());
    }
    let (owner, name, _) = parse_repo(&repository.url)?;
    let jobs = jobs.inner().clone();
    begin_job(&jobs, "Another job is running", true)?;
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
                    "build",
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
                    if prs
                        .first()
                        .is_some_and(|first: &CommunityPullRequest| first.base_sha != pr.base_sha)
                    {
                        return Err(format!("PR #{} is based on a different target commit; select PRs with the same pinned base", pr.number));
                    }
                } else {
                    base_branch = Some(pr.base_branch.clone());
                }
                prs.push(pr);
            }
            ensure_job_not_cancelled(&jobs)?;
            let base_sha = prs
                .first()
                .map(|pr| pr.base_sha.clone())
                .ok_or("No base commit")?;
            if base_sha.len() != 40 || !base_sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("GitHub returned an invalid base SHA".into());
            }
            let branch = base_branch.clone().unwrap_or_else(|| "unknown".into());
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
            history_db.execute("INSERT OR IGNORE INTO community_build_configs(signature,repo_id,name,base_branch,base_sha,build_type,jobs,created_at,compiler,clean) VALUES(?,?,?,?,?,?,?,?,?,?)", params![signature, repository_id, format!("Community Combined Build — PR {}", pull_requests.iter().map(|selection| format!("#{}", selection.number)).collect::<Vec<_>>().join(" + ")), branch, base_sha, options.build_type, options.jobs, now(), compiler, options.clean]).map_err(|e| e.to_string())?;
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
            let paths = state_paths(&app)?;
            let work_root = paths.cache.join("community-worktrees");
            fs::create_dir_all(&work_root).map_err(|e| e.to_string())?;
            let worktree = work_root.join(format!(
                "repo-{repository_id}-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let source_text = source.to_string_lossy().to_string();
            let work_text = worktree.to_string_lossy().to_string();
            emit(
                &app,
                "build",
                "Preparing repository",
                "Creating an isolated Git clone; the registered source checkout remains untouched",
                false,
                true,
            );
            git_cancellable(
                &app,
                &jobs,
                None,
                &[
                    "clone",
                    "--no-hardlinks",
                    "--no-checkout",
                    &source_text,
                    &work_text,
                ],
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
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["cat-file", "-e", &format!("{base_sha}^{{commit}}")],
            )
            .map_err(|_| {
                "Pinned PR base commit is no longer reachable from the target branch".to_string()
            })?;
            git_cancellable(
                &app,
                &jobs,
                Some(&worktree),
                &["checkout", "--detach", &base_sha],
            )?;
            for pr in &prs {
                ensure_job_not_cancelled(&jobs)?;
                let reference = format!("refs/studio/pr/{}", pr.number);
                let source_ref = format!("+refs/pull/{}/head:{}", pr.number, reference);
                emit(
                    &app,
                    "build",
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
                    "build",
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
            emit(&app, "build", "Checking merge conflicts", format!("All {} selected PRs applied in the chosen order. Git merge success does not establish runtime or gameplay compatibility.", prs.len()), false, true);
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
        finish_job(&app, &jobs, "build", result);
    });
    Ok(())
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
    github_json("https://api.github.com/repos/KytyPS5/KytyPS5/forks?sort=updated&per_page=30")
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
        present: command_exists(n),
    })
    .collect();
    for name in [
        "Qt6Core",
        "Qt6Widgets",
        "Qt6Network",
        "Qt6Concurrent",
        "Qt6DBus",
        "vulkan",
        "x11",
        "wayland-client",
        "libpulse",
        "libudev",
    ] {
        dependencies.push(Dependency {
            name: name.into(),
            present: Command::new("pkg-config")
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
    let _ = app.emit(
        "studio-job",
        JobEvent {
            kind: kind.into(),
            phase: phase.into(),
            line: line.into(),
            done,
            success,
        },
    );
}
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
    let logfile = log_path
        .map(|path| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map(|f| Arc::new(Mutex::new(f)))
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    if let Some(file) = &logfile {
        let _ = writeln!(file.lock().unwrap(), "$ {} {}", program, args.join(" "));
    }
    emit(
        app,
        kind,
        phase,
        format!("$ {} {}", program, args.join(" ")),
        false,
        true,
    );
    let mut c = Command::new(program);
    c.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    c.process_group(0);
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    let pid = child.id() as i32;
    jobs.pid.store(pid, Ordering::SeqCst);
    if jobs.cancel_requested.load(Ordering::SeqCst) {
        let _ = terminate_process_group(pid);
    }
    let stderr = child.stderr.take().ok_or("Missing stderr")?;
    let a = app.clone();
    let k = kind.to_string();
    let p = phase.to_string();
    let error_log = logfile.clone();
    let err = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(file) = &error_log {
                let _ = writeln!(file.lock().unwrap(), "[stderr] {line}");
            }
            emit(&a, &k, &p, line, false, true)
        }
    });
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(file) = &logfile {
                let _ = writeln!(file.lock().unwrap(), "[stdout] {line}");
            }
            emit(app, kind, phase, line, false, true)
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = err.join();
    jobs.pid.store(0, Ordering::SeqCst);
    if jobs.cancel_requested.load(Ordering::SeqCst) {
        return Err("Job cancelled".into());
    }
    if !status.success() {
        return Err(format!("{} failed with {}", program, status));
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
#[tauri::command]
fn start_clone(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    id: i64,
) -> Result<(), String> {
    let c = db(&app)?;
    let r = repo(&c, id)?;
    let p = state_paths(&app)?;
    let target = source_dir(&p, id);
    if target.exists() {
        return Err("Source checkout already exists".into());
    }
    let jobs = jobs.inner().clone();
    begin_job(&jobs, "Another job is running", true)?;
    thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
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
            ensure_job_not_cancelled(&jobs)?;
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
        if result.is_err() && target.exists() {
            let _ = fs::remove_dir_all(&target);
        }
        finish_job(
            &app,
            &jobs,
            "clone",
            result.map(|()| "Repository ready".into()),
        );
    });
    Ok(())
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
fn check_updates(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    id: i64,
) -> Result<String, String> {
    begin_job(&jobs, "A build or update job is already running", false)?;
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
fn inspect_build_source(
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
    let cache_dir = state_paths(&app)?
        .cache
        .join("builds")
        .join(repository_id.to_string())
        .join(format!(
            "{}-{build_type}",
            &commit_sha[..commit_sha.len().min(12)]
        ));
    let compiler = "clang";
    let mut cmake_arguments = vec![
        "-G Ninja".into(),
        format!("-DCMAKE_BUILD_TYPE={build_type}"),
        format!("-DCMAKE_C_COMPILER={compiler}"),
        format!("-DCMAKE_CXX_COMPILER={compiler}++"),
    ];
    cmake_arguments.extend(linux_x86_64_cmake_cpu_flags(
        std::env::consts::OS,
        std::env::consts::ARCH,
    ));
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
    let compiler = options.compiler.clone().unwrap_or_else(|| "clang".into());
    if compiler != "clang" {
        return Err("Unsupported compiler selection".into());
    }
    Ok(compiler)
}
const BUILD_DEPENDENCIES: &[&str] = &[
    "cmake",
    "ninja",
    "clang",
    "clang++",
    "pkg-config",
    "python3",
];
const BUILD_PKG_CONFIG_DEPENDENCIES: &[&str] =
    &["Qt6Concurrent", "Qt6Network", "Qt6Widgets", "Qt6DBus"];
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
fn linux_x86_64_cmake_cpu_flags(target_os: &str, target_arch: &str) -> Vec<String> {
    if target_os == "linux" && target_arch == "x86_64" {
        vec!["-DCMAKE_CXX_FLAGS=-msse4.1".into()]
    } else {
        Vec::new()
    }
}
fn verify_build_dependencies(app: &tauri::AppHandle) -> Result<(), String> {
    emit(
        app,
        "build",
        "Dependency verification",
        "Checking required Linux build tools",
        false,
        true,
    );
    let mut missing = missing_build_dependencies(command_exists);
    if !command_exists("glslangValidator") && !command_exists("glslang") {
        missing.push("glslangValidator or glslang");
    }
    let missing_packages = if command_exists("pkg-config") {
        missing_pkg_config_dependencies(|name| {
            Command::new("pkg-config")
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
            "Qt6DBus" => "Qt6DBus development files",
            _ => name,
        }));
        Err(format!(
            "Missing build dependencies: {}. Install these with your distribution package manager, then retry.",
            issues.join(", ")
        ))
    }
}
fn build_and_register(
    app: &tauri::AppHandle,
    jobs: &Jobs,
    repository_id: i64,
    source: &Path,
    commit: &str,
    options: &BuildOptions,
    compiler: &str,
) -> Result<PathBuf, String> {
    ensure_job_not_cancelled(jobs)?;
    verify_build_dependencies(app)?;
    let paths = state_paths(app)?;
    let build_dir = paths
        .cache
        .join("builds")
        .join(repository_id.to_string())
        .join(format!(
            "{}-{}",
            &commit[..commit.len().min(12)],
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
                .as_millis()
        ));
    if build_dir.exists() && options.clean {
        fs::remove_dir_all(&build_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;
    let build_dir_s = build_dir.display().to_string();
    let source_s = source.display().to_string();
    let cxx = format!("{compiler}++");
    let mut conf = vec![
        "-S".into(),
        source_s,
        "-B".into(),
        build_dir_s.clone(),
        "-G".into(),
        "Ninja".into(),
        format!("-DCMAKE_BUILD_TYPE={}", options.build_type),
        format!("-DCMAKE_C_COMPILER={compiler}"),
        format!("-DCMAKE_CXX_COMPILER={cxx}"),
    ];
    conf.extend(linux_x86_64_cmake_cpu_flags(
        std::env::consts::OS,
        std::env::consts::ARCH,
    ));
    let log_path = build_dir.join("build.log");
    fs::write(&log_path, b"").map_err(|e| e.to_string())?;
    execute(
        app,
        jobs,
        "build",
        "Configure",
        "cmake",
        &conf,
        None,
        Some(&log_path),
    )?;
    execute(
        app,
        jobs,
        "build",
        "Compile",
        "cmake",
        &[
            "--build".into(),
            build_dir_s.clone(),
            "--target".into(),
            "launcher".into(),
            "--parallel".into(),
            options.jobs.to_string(),
        ],
        None,
        Some(&log_path),
    )?;
    execute(
        app,
        jobs,
        "build",
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
    emit(
        app,
        "build",
        "Validation",
        "Checking the installed emulator executable and game CLI support",
        false,
        true,
    );
    verify_emulator(&install.join("kyty_emulator"))?;
    ensure_job_not_cancelled(jobs)?;
    fs::copy(&log_path, install.join("build.log")).map_err(|e| e.to_string())?;
    let install_size_bytes = directory_size_bytes(&install);
    ensure_job_not_cancelled(jobs)?;
    paths.db()?.execute(
        "INSERT OR IGNORE INTO builds(repo_id,commit_sha,build_type,path,created_at,install_size_bytes,validation_status) VALUES(?,?,?,?,?,?,'verified')",
        params![repository_id, commit, options.build_type, install.display().to_string(), now(), install_size_bytes.map(|size| size.min(i64::MAX as u64) as i64)],
    ).map_err(|e| e.to_string())?;
    Ok(install)
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
) -> Result<(), String> {
    let compiler = validate_build_options(&options)?;
    let c = db(&app)?;
    let r = repo(&c, id)?;
    if !["official", "kyty-cmake"].contains(&r.adapter.as_str()) {
        return Err("No verified build adapter for this checkout".into());
    }
    let source = PathBuf::from(r.source_path.ok_or("Clone the repository first")?);
    let commit = git(Some(&source), &["rev-parse", "HEAD"])?;
    let branch = checked_out_branch(&source)?;
    let jobs = jobs.inner().clone();
    begin_job(&jobs, "Another job is running", true)?;
    thread::spawn(move || {
        let result = build_and_register(&app, &jobs, id, &source, &commit, &options, &compiler)
            .and_then(|path| {
                mark_repository_build_commit(&app, id, &commit, &branch)?;
                Ok(path)
            });
        finish_job(
            &app,
            &jobs,
            "build",
            result.map(|path| format!("Build verified and registered at {}", path.display())),
        );
    });
    Ok(())
}
#[tauri::command]
fn start_update_build(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Arc<Jobs>>,
    id: i64,
    options: BuildOptions,
) -> Result<(), String> {
    let compiler = validate_build_options(&options)?;
    let c = db(&app)?;
    let repository = repo(&c, id)?;
    if !["official", "kyty-cmake"].contains(&repository.adapter.as_str()) {
        return Err("No verified build adapter for this checkout".into());
    }
    let source = PathBuf::from(repository.source_path.ok_or("Clone the repository first")?);
    let jobs = jobs.inner().clone();
    begin_job(&jobs, "Another job is running", true)?;
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let branch = checked_out_branch(&source)?;
            if !repository.branch.is_empty() && branch != repository.branch {
                return Err(format!(
                    "Checked out branch '{branch}' differs from tracked branch '{}'",
                    repository.branch
                ));
            }
            if !git(Some(&source), &["status", "--porcelain"])?.is_empty() {
                return Err(
                    "Local source changes found. Commit or remove them before building an update"
                        .into(),
                );
            }
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
                if verify_emulator(&Path::new(&path).join("kyty_emulator")).is_ok() {
                    mark_repository_build_commit(&app, id, &commit, &branch)?;
                    return Ok(format!(
                        "Commit {} is already installed and verified; no rebuild was needed",
                        &commit[..12]
                    ));
                }
            }
            let path = build_and_register(&app, &jobs, id, &source, &commit, &options, &compiler)?;
            mark_repository_build_commit(&app, id, &commit, &branch)?;
            Ok(format!(
                "Update build verified at {}. The active build was left unchanged.",
                path.display()
            ))
        })();
        finish_job(&app, &jobs, "build", result);
    });
    Ok(())
}
#[tauri::command]
fn cancel_job(jobs: tauri::State<'_, Arc<Jobs>>) -> Result<(), String> {
    let _control = jobs.control.lock().map_err(|error| error.to_string())?;
    if !jobs.active.load(Ordering::SeqCst) || !jobs.cancellable.load(Ordering::SeqCst) {
        return Err("No cancellable job is running".into());
    }
    jobs.cancel_requested.store(true, Ordering::SeqCst);
    let pid = jobs.pid.load(Ordering::SeqCst);
    if pid <= 0 {
        return Ok(());
    }
    if let Err(error) = terminate_process_group(pid) {
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error.to_string());
        }
    }
    Ok(())
}
#[tauri::command]
fn validate_build(app: tauri::AppHandle, id: i64) -> Result<String, String> {
    let c = db(&app)?;
    let b = build(&c, id)?;
    let validation = verify_emulator(&Path::new(&b.path).join("kyty_emulator"));
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
    verify_emulator(&Path::new(&b.path).join("kyty_emulator"))?;
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
fn emulator_help(executable: &Path) -> Result<String, String> {
    let metadata =
        fs::metadata(executable).map_err(|_| "Emulator executable is missing".to_string())?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err("Emulator path is not an executable file".into());
    }
    let mut child = Command::new(executable)
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("Could not run emulator validation: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.wait();
            return Err("Emulator --help timed out after 5 seconds".into());
        }
        thread::sleep(Duration::from_millis(25));
    };
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("Emulator --help exited with {status}"));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "Emulator --help output is not valid UTF-8".to_string())
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
    let install_size = if p.is_dir() {
        walkdir::WalkDir::new(&p)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.metadata().ok())
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len() as i64)
            .sum()
    } else {
        fs::metadata(&p)
            .map(|metadata| metadata.len() as i64)
            .unwrap_or(0)
    };
    let c = db(&app)?;
    c.execute(
        "INSERT OR IGNORE INTO games(path,title,title_id,game_version,icon_path,install_size) VALUES(?,?,?,?,?,?)",
        params![p.display().to_string(), title, title_id, game_version, icon_path, install_size],
    )
    .map_err(|e| e.to_string())?;
    c.execute(
        "UPDATE games SET title=CASE WHEN title_id IS NULL AND ? IS NOT NULL THEN ? ELSE title END, title_id=COALESCE(title_id,?), game_version=COALESCE(game_version,?), icon_path=COALESCE(icon_path,?), install_size=COALESCE(install_size,?) WHERE path=?",
        params![title_id, title, title_id, game_version, icon_path, install_size, p.display().to_string()],
    ).map_err(|e| e.to_string())?;
    let id = c
        .query_row(
            "SELECT id FROM games WHERE path=?",
            [p.display().to_string()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    game(&c, id)
}
fn supported_game_path(path: &Path) -> bool {
    if path.is_dir() {
        path.join("eboot.bin").is_file()
    } else {
        if path.file_name().and_then(|s| s.to_str()) == Some("eboot.bin") {
            true
        } else {
            match path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("elf") => is_elf_file(path),
                Some("zar") => path.is_file(),
                _ => false,
            }
        }
    }
}
fn is_elf_file(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).is_ok() && magic == *b"\x7fELF"
}
fn game_metadata(path: &Path) -> (String, Option<String>, Option<String>, Option<String>) {
    let root = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let meta = root.join("sce_sys/param.json");
    let icon = root.join("sce_sys/icon0.png");
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
fn read_game_icon(app: tauri::AppHandle, game_id: i64) -> Result<Option<Vec<u8>>, String> {
    let c = db(&app)?;
    let game = game(&c, game_id)?;
    let Some(icon) = game.icon_path else {
        return Ok(None);
    };
    let Some(root) = game_artwork_root(Path::new(&game.path)) else {
        return Ok(None);
    };
    Ok(read_game_png(Path::new(&icon), &root, 4 * 1024 * 1024))
}
#[tauri::command]
fn read_game_background(app: tauri::AppHandle, game_id: i64) -> Result<Option<Vec<u8>>, String> {
    let c = db(&app)?;
    let game = game(&c, game_id)?;
    let Some(root) = game_artwork_root(Path::new(&game.path)) else {
        return Ok(None);
    };
    Ok(read_game_png(
        &root.join("sce_sys/pic0.png"),
        &root,
        32 * 1024 * 1024,
    ))
}
fn game_artwork_root(game_path: &Path) -> Option<PathBuf> {
    if game_path.is_dir() {
        Some(game_path.to_path_buf())
    } else if game_path.file_name().and_then(|name| name.to_str()) == Some("eboot.bin")
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
    if !path.starts_with(&root) || !path.is_file() {
        return None;
    }
    let metadata = fs::metadata(&path).ok()?;
    if metadata.len() > maximum_size {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    bytes.starts_with(b"\x89PNG\r\n\x1a\n").then_some(bytes)
}
#[tauri::command]
fn scan_games(app: tauri::AppHandle, path: String) -> Result<usize, String> {
    let root = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("Select a directory".into());
    }
    let mut count = 0;
    for entry in walkdir::WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        let hit = entry.file_name() == "eboot.bin" && p.is_file();
        let archive = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("zar"))
            .unwrap_or(false)
            && p.is_file();
        let elf = p
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("elf"))
            .unwrap_or(false)
            && is_elf_file(p);
        if hit || archive || elf {
            let candidate = if hit { p.parent().unwrap() } else { p };
            if add_game(app.clone(), candidate.display().to_string()).is_ok() {
                count += 1
            }
        }
    }
    Ok(count)
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
fn get_profile_capabilities(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<ProfileCapabilities, String> {
    let c = db(&app)?;
    capabilities_for_build(&build_for_game(&c, game_id)?)
}
fn validate_profile_values(profile: &Profile) -> Result<(), String> {
    for value in [profile.screen_width, profile.screen_height]
        .into_iter()
        .flatten()
    {
        if !(320..=7680).contains(&value) {
            return Err("Resolution must be between 320 and 7680".into());
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
fn set_profile(app: tauri::AppHandle, game_id: i64, profile: Profile) -> Result<(), String> {
    validate_profile_values(&profile)?;
    let c = db(&app)?;
    game(&c, game_id)?;
    let capabilities = capabilities_for_build(&build_for_game(&c, game_id)?)?;
    if profile.screen_width.is_some() && !capabilities.screen_width
        || profile.screen_height.is_some() && !capabilities.screen_height
        || profile.present_mode.is_some() && !capabilities.present_mode
        || profile.perf_overlay && !capabilities.perf_overlay
        || profile.fullscreen && !capabilities.fullscreen
        || profile.controller_color.is_some() && !capabilities.controller_color
        || profile.controller_volume.is_some() && !capabilities.controller_volume
        || profile.controller_vibration.is_some() && !capabilities.controller_vibration
        || profile.keyboard_player.is_some() && !capabilities.keyboard_player
    {
        return Err(
            "This emulator build does not support one or more selected profile options".into(),
        );
    }
    c.execute("INSERT INTO profiles(game_id,screen_width,screen_height,present_mode,perf_overlay,fullscreen,controller_color,controller_volume,controller_vibration,keyboard_player) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(game_id) DO UPDATE SET screen_width=excluded.screen_width,screen_height=excluded.screen_height,present_mode=excluded.present_mode,perf_overlay=excluded.perf_overlay,fullscreen=excluded.fullscreen,controller_color=excluded.controller_color,controller_volume=excluded.controller_volume,controller_vibration=excluded.controller_vibration,keyboard_player=excluded.keyboard_player",params![game_id,profile.screen_width,profile.screen_height,profile.present_mode,profile.perf_overlay,profile.fullscreen,profile.controller_color,profile.controller_volume,profile.controller_vibration,profile.keyboard_player]).map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
fn launch_game(app: tauri::AppHandle, game_id: i64) -> Result<(), String> {
    let c = db(&app)?;
    let g = game(&c, game_id)?;
    let game_profile = profile(&c, game_id)?;
    let build_id = if let Some(id) = g.build_id {
        id
    } else {
        c.query_row("SELECT id FROM builds WHERE active=1 LIMIT 1", [], |r| {
            r.get(0)
        })
        .map_err(|_| "Select an active emulator build".to_string())?
    };
    let b = build(&c, build_id)?;
    let capabilities = capabilities_for_build(&b)?;
    if game_profile.screen_width.is_some() && !capabilities.screen_width
        || game_profile.screen_height.is_some() && !capabilities.screen_height
        || game_profile.present_mode.is_some() && !capabilities.present_mode
        || game_profile.perf_overlay && !capabilities.perf_overlay
        || game_profile.fullscreen && !capabilities.fullscreen
        || game_profile.controller_color.is_some() && !capabilities.controller_color
        || game_profile.controller_volume.is_some() && !capabilities.controller_volume
        || game_profile.controller_vibration.is_some() && !capabilities.controller_vibration
        || game_profile.keyboard_player.is_some() && !capabilities.keyboard_player
    {
        return Err("The selected emulator build no longer supports this game's saved profile. Clear unsupported profile options before launching.".into());
    }
    let exe = Path::new(&b.path).join("kyty_emulator");
    let game_path = fs::canonicalize(&g.path)
        .map_err(|_| "The registered game path is missing or inaccessible".to_string())?;
    if !supported_game_path(&game_path) {
        return Err(
            "The registered game path no longer contains a supported eboot.bin, ELF, or ZArchive"
                .into(),
        );
    }
    if !exe.is_file()
        || fs::metadata(&exe)
            .map(|m| m.permissions().mode() & 0o111 == 0)
            .unwrap_or(true)
    {
        return Err("Emulator or game path no longer exists".into());
    }
    let log = state_paths(&app)?.data.join("logs");
    fs::create_dir_all(&log).map_err(|e| e.to_string())?;
    let log_path = log.join(format!("game-{}-{}.log", game_id, now()));
    let logfile = Arc::new(Mutex::new(
        fs::File::create(&log_path).map_err(|e| e.to_string())?,
    ));
    let started = now();
    c.execute(
        "INSERT INTO sessions(game_id,build_id,started_at,log_path) VALUES(?,?,?,?)",
        params![game_id, build_id, started, log_path.display().to_string()],
    )
    .map_err(|e| e.to_string())?;
    let session_id = c.last_insert_rowid();
    let mut command = Command::new(&exe);
    command.arg("--game").arg(&game_path);
    if let Some(width) = game_profile.screen_width {
        command.arg("--screen-width").arg(width.to_string());
    }
    if let Some(height) = game_profile.screen_height {
        command.arg("--screen-height").arg(height.to_string());
    }
    if let Some(mode) = game_profile.present_mode {
        command.arg("--present-mode").arg(mode);
    }
    if game_profile.perf_overlay {
        command.arg("--perf-overlay").arg("true");
    }
    if game_profile.fullscreen {
        command.arg("--fullscreen");
    }
    if let Some(color) = game_profile.controller_color {
        command.arg("--controller-color").arg(color);
    }
    if let Some(volume) = game_profile.controller_volume {
        command.arg("--controller-volume").arg(volume.to_string());
    }
    if let Some(vibration) = game_profile.controller_vibration {
        command
            .arg("--controller-vibration")
            .arg(vibration.to_string());
    }
    if let Some(player) = game_profile.keyboard_player {
        command.arg("--keyboard-player").arg(player.to_string());
    }
    command
        .current_dir(&b.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let pid = child.id();
    let process = (0..10).find_map(|_| {
        let stat = proc_stat(pid);
        if stat.is_none() {
            thread::sleep(Duration::from_millis(10));
        }
        stat
    });
    let tracking = if let Some(stat) = process {
        c.execute(
            "UPDATE sessions SET process_pid=?,process_start_ticks=? WHERE id=?",
            params![pid as i64, stat.start_ticks as i64, session_id],
        )
    } else {
        c.execute(
            "UPDATE sessions SET process_pid=? WHERE id=?",
            params![pid as i64, session_id],
        )
    };
    if let Err(error) = tracking {
        let _ = child.kill();
        let _ = child.wait();
        let _ = c.execute(
            "UPDATE sessions SET ended_at=?,outcome='tracking_error' WHERE id=?",
            params![now(), session_id],
        );
        return Err(format!(
            "Emulator started as PID {pid}, but session tracking failed: {error}"
        ));
    }
    emit(&app, "game", "Running", format!("PID {pid}"), false, true);
    thread::spawn(move || {
        let result = (|| -> Result<(Option<i32>, String), String> {
            let stderr = child.stderr.take().unwrap();
            let lf = logfile.clone();
            let a = app.clone();
            let err = thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    use std::io::Write;
                    let _ = writeln!(lf.lock().unwrap(), "[stderr] {line}");
                    emit(&a, "game", "stderr", line, false, true)
                }
            });
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    use std::io::Write;
                    let _ = writeln!(logfile.lock().unwrap(), "[stdout] {line}");
                    emit(&app, "game", "stdout", line, false, true)
                }
            }
            let status = child.wait().map_err(|e| e.to_string())?;
            let _ = err.join();
            let outcome = if status.success() {
                "normal"
            } else if status.code().is_some() {
                "error"
            } else {
                "crash"
            };
            Ok((status.code(), outcome.into()))
        })();
        if let Ok(c) = db(&app) {
            let (code, outcome) = result
                .as_ref()
                .map(|(code, outcome)| (*code, outcome.as_str()))
                .unwrap_or((None, "launch_error"));
            let ended = now();
            let _ = c.execute(
                "UPDATE sessions SET ended_at=?,exit_code=?,outcome=? WHERE id=?",
                params![ended, code, outcome, session_id],
            );
            let _ = c.execute(
                "UPDATE games SET play_seconds=play_seconds+?,last_played=? WHERE id=?",
                params![ended - started, started, game_id],
            );
        }
        emit(
            &app,
            "game",
            "Complete",
            result
                .as_ref()
                .map(|(_, o)| o.clone())
                .unwrap_or_else(|e| e.clone()),
            true,
            result.is_ok(),
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

    let clock_ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let now_instant = Instant::now();
    let now_wall = now();
    let mut previous = samples.0.lock().map_err(|e| e.to_string())?;
    let mut active_ids = HashSet::new();
    let mut lost: Vec<(i64, Option<i64>, &'static str)> = Vec::new();
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
                lost.push((session_id, None, "launch_interrupted"));
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

        let expected_executable = Path::new(&build_path).join("kyty_emulator");
        let process_matches =
            expected_start.is_none() && process_executable_matches(pid, &expected_executable);
        let stat = proc_stat(pid).filter(|stat| match expected_start {
            Some(expected) => expected as u64 == stat.start_ticks,
            None => process_matches,
        });
        let Some(stat) = stat else {
            if process_matches {
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
                    status: "Reading process telemetry".into(),
                });
                continue;
            }
            let sample = previous.entry(session_id).or_default();
            sample.missing_samples = sample.missing_samples.saturating_add(1);
            let status = if sample.missing_samples >= 2 {
                lost.push((session_id, Some(started_at), "process_lost"));
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
        if expected_start.is_none() {
            conn.execute(
                "UPDATE sessions SET process_start_ticks=? WHERE id=? AND process_start_ticks IS NULL",
                params![stat.start_ticks as i64, session_id],
            ).map_err(|error| error.to_string())?;
        }
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
        active_ids.contains(session_id) && !lost.iter().any(|(lost_id, _, _)| lost_id == session_id)
    });
    drop(previous);
    if !lost.is_empty() {
        let conn = db(&app)?;
        for (session_id, started_at, outcome) in lost {
            let updated = conn
                .execute(
                    "UPDATE sessions SET ended_at=?,outcome=? WHERE id=? AND ended_at IS NULL",
                    params![now_wall, outcome, session_id],
                )
                .map_err(|e| e.to_string())?;
            if updated > 0 {
                if let Some(started_at) = started_at {
                    conn.execute(
                        "UPDATE games SET play_seconds=play_seconds+?,last_played=? WHERE id=(SELECT game_id FROM sessions WHERE id=?)",
                        params![now_wall.saturating_sub(started_at).max(0), started_at, session_id],
                    ).map_err(|e| e.to_string())?;
                }
            }
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
    let pid = pid
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 1)
        .ok_or("The session has no verified emulator process ID")?;
    let expected_executable = Path::new(&build_path).join("kyty_emulator");
    let actual = proc_stat(pid as u32).ok_or("The emulator process has already exited")?;
    if expected_start.is_some_and(|expected| expected as u64 != actual.start_ticks)
        || !process_executable_matches(pid as u32, &expected_executable)
    {
        return Err("The process no longer matches this registered emulator session".into());
    }
    terminate_process_group(pid)
        .map_err(|error| format!("Could not stop emulator process: {error}"))?;
    Ok(format!("Stop signal sent to emulator session {session_id}"))
}
#[tauri::command]
fn recent_sessions(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let c = db(&app)?;
    let mut s=c.prepare("SELECT sessions.id,sessions.game_id,sessions.build_id,games.title,sessions.started_at,sessions.ended_at,sessions.exit_code,sessions.outcome,sessions.log_path,builds.commit_sha,builds.build_type,sessions.process_pid FROM sessions JOIN games ON games.id=sessions.game_id JOIN builds ON builds.id=sessions.build_id ORDER BY sessions.id DESC LIMIT 30").map_err(|e|e.to_string())?;
    let rows=s.query_map([],|r|Ok(serde_json::json!({"id":r.get::<_,i64>(0)?,"game_id":r.get::<_,i64>(1)?,"build_id":r.get::<_,i64>(2)?,"title":r.get::<_,String>(3)?,"started_at":r.get::<_,i64>(4)?,"ended_at":r.get::<_,Option<i64>>(5)?,"exit_code":r.get::<_,Option<i32>>(6)?,"outcome":r.get::<_,Option<String>>(7)?,"log_path":r.get::<_,String>(8)?,"commit_sha":r.get::<_,String>(9)?,"build_type":r.get::<_,String>(10)?,"pid":r.get::<_,Option<i64>>(11)?}))).map_err(|e|e.to_string())?;
    let result = rows.map(|r| r.map_err(|e| e.to_string())).collect();
    result
}
#[tauri::command]
fn read_log(path: String, app: tauri::AppHandle) -> Result<String, String> {
    let base = fs::canonicalize(state_paths(&app)?.data.join("logs"))
        .map_err(|e| format!("Managed log directory is unavailable: {e}"))?;
    let p = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !p.starts_with(&base) || !p.is_file() {
        return Err("Log path is outside the managed directory".into());
    }
    read_bounded_tail(&p, 4 * 1024 * 1024)
}
#[tauri::command]
fn export_log(path: String, destination: String, app: tauri::AppHandle) -> Result<(), String> {
    let log_root = fs::canonicalize(state_paths(&app)?.data.join("logs"))
        .map_err(|error| format!("Managed log directory is unavailable: {error}"))?;
    let source = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !source.starts_with(&log_root) || !source.is_file() {
        return Err("Log path is outside the managed log directory".into());
    }
    copy_file_atomically(&source, Path::new(&destination), 512 * 1024 * 1024)
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
        fs::rename(&temporary, &destination)
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
    if !path.starts_with(&root) || !path.is_file() {
        return Err("Build log is outside the registered installation".into());
    }
    read_bounded_tail(&path, 4 * 1024 * 1024)
}
#[tauri::command]
fn get_setting(app: tauri::AppHandle, key: String) -> Result<Option<String>, String> {
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
        "github_token",
    ]
    .contains(&key.as_str())
    {
        return Err("Unsupported setting".into());
    }
    if key == "update_interval" && !valid_update_interval(&value) {
        return Err("Update interval must be startup-only, 6 hours, 24 hours, or weekly".into());
    }
    if key == "auto_update_mode" && !valid_auto_update_mode(&value) {
        return Err("Update behavior must be notify or build".into());
    }
    db(&app)?.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(|e|e.to_string())?;
    Ok(())
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
fn desktop_notification(title: String, body: String) -> Result<(), String> {
    let title = notification_text(&title, 120);
    let body = notification_text(&body, 2048);
    let status = Command::new("notify-send")
        .arg("--app-name=KytyPS5 Studio")
        .arg("--")
        .arg(title)
        .arg(body)
        .status()
        .map_err(|error| format!("Desktop notifications are unavailable: {error}"))?;
    if !status.success() {
        return Err(format!("Desktop notification command exited with {status}"));
    }
    Ok(())
}
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
    let p = Paths::new().expect("Cannot create application directories");
    p.db().expect("Cannot initialize database");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(p)
        .manage(Arc::new(Jobs {
            pid: AtomicI32::new(0),
            active: AtomicBool::new(false),
            cancel_requested: AtomicBool::new(false),
            cancellable: AtomicBool::new(false),
            control: Mutex::new(()),
        }))
        .manage(ProcessSamples::default())
        .invoke_handler(tauri::generate_handler![
            list_repos,
            open_repository_folder,
            list_builds,
            open_build_folder,
            validate_build,
            list_games,
            game_compatibility,
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
            inspect_graphics,
            start_clone,
            inspect_repository,
            check_updates,
            start_build,
            start_update_build,
            cancel_job,
            activate_build,
            add_game,
            open_game_folder,
            read_game_icon,
            read_game_background,
            scan_games,
            set_game_build,
            toggle_favorite,
            get_profile,
            get_profile_capabilities,
            set_profile,
            launch_game,
            process_metrics,
            stop_game_session,
            recent_sessions,
            read_log,
            export_log,
            read_build_log,
            get_setting,
            set_setting,
            desktop_notification
        ])
        .run(tauri::generate_context!())
        .expect("Tauri error");
}
#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn process_group_termination_stops_the_requested_child_group() {
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        terminate_process_group(child.id() as i32).unwrap();
        assert!(!child.wait().unwrap().success());
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
        let profile_columns: i64 = db.query_row("SELECT count(*) FROM pragma_table_info('profiles') WHERE name IN ('controller_color','controller_volume','controller_vibration','keyboard_player')", [], |r| r.get(0)).unwrap();
        let process_columns: i64 = db.query_row("SELECT count(*) FROM pragma_table_info('sessions') WHERE name IN ('process_pid','process_start_ticks')", [], |r| r.get(0)).unwrap();
        let build_columns: i64 = db.query_row("SELECT count(*) FROM pragma_table_info('builds') WHERE name IN ('install_size_bytes','validation_status')", [], |r| r.get(0)).unwrap();
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
        assert_eq!(version, 9);
        assert_eq!(columns, 4);
        assert_eq!(profile_columns, 4);
        assert_eq!(process_columns, 2);
        assert_eq!(build_columns, 2);
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
        assert_eq!(version, 9);
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
        let linked = root.join("sce_sys/linked.png");
        std::os::unix::fs::symlink(&outside, &linked).unwrap();
        assert!(read_game_png(&linked, &root, 64).is_none());
        fs::remove_dir_all(dir).unwrap();
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
    fn profile_capabilities_are_derived_from_emulator_help() {
        let capabilities = profile_capabilities(
            "Options:\n  --screen-width <num> Window width\n  --present-mode <value> Fifo\n  --fullscreen Run fullscreen\n  --controller-color <#RRGGBB> Controller light\n  --controller-volume <0-100> Speaker volume\n  --controller-vibration <0-100> Vibration\n  --keyboard-player <0-4> Keyboard player\n",
        );
        assert!(capabilities.screen_width);
        assert!(!capabilities.screen_height);
        assert!(capabilities.present_mode);
        assert!(!capabilities.perf_overlay);
        assert!(capabilities.fullscreen);
        assert!(capabilities.controller_color);
        assert!(capabilities.controller_volume);
        assert!(capabilities.controller_vibration);
        assert!(capabilities.keyboard_player);
    }
    #[test]
    fn executable_validation_requires_a_successful_game_help_response() {
        let dir = std::env::temp_dir().join(format!("kyty-emulator-verify-{}", now()));
        fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("kyty_emulator");
        fs::write(
            &executable,
            "#!/bin/sh\nprintf 'usage: kyty_emulator --game <dir>\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify_emulator(&executable).is_ok());
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
        assert_eq!(validate_build_options(&options).unwrap(), "clang");
        let invalid = BuildOptions {
            build_type: "Other".into(),
            ..options
        };
        assert!(validate_build_options(&invalid).is_err());
    }
    #[test]
    fn build_dependency_preflight_reports_only_missing_tools() {
        assert!(missing_build_dependencies(|_| true).is_empty());
        assert_eq!(
            missing_build_dependencies(|name| name != "ninja" && name != "python3"),
            vec!["ninja", "python3"]
        );
        assert_eq!(
            missing_pkg_config_dependencies(|name| name != "Qt6DBus"),
            vec!["Qt6DBus"]
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
        fs::write(dir.join("folder/eboot.bin"), b"elf").unwrap();
        fs::write(dir.join("game.ZAR"), b"archive").unwrap();
        fs::write(dir.join("game.elf"), b"\x7fELF\x02\x01").unwrap();
        fs::write(dir.join("fake.elf"), b"not an executable").unwrap();
        fs::write(dir.join("notes.txt"), b"no").unwrap();
        assert!(supported_game_path(&dir.join("folder")));
        assert!(supported_game_path(&dir.join("game.ZAR")));
        assert!(supported_game_path(&dir.join("game.elf")));
        assert!(!supported_game_path(&dir.join("fake.elf")));
        assert!(!supported_game_path(&dir.join("notes.txt")));
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
