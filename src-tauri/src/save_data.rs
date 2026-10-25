//! Conservative manager for the on-disk layouts implemented by KytyPS5.
//! The emulator resolves `_SaveData` relative to its working directory, which
//! Studio sets to the selected build directory.
use super::*;
use serde::Serialize;

const MAX_SAVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Serialize, Clone)]
struct SaveDataProgress {
    game_id: i64,
    operation: String,
    phase: String,
    completed: u64,
    total: u64,
}

fn emit_progress(
    app: &tauri::AppHandle,
    game_id: i64,
    operation: &str,
    phase: &str,
    completed: u64,
    total: u64,
) {
    let _ = app.emit(
        "save-data-progress",
        SaveDataProgress {
            game_id,
            operation: operation.to_string(),
            phase: phase.to_string(),
            completed,
            total,
        },
    );
}
fn nonce() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        now(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

#[derive(Clone, Serialize)]
pub(super) struct SaveEntry {
    id: String,
    title_id: String,
    user_id: Option<String>,
    slot: String,
    build_id: i64,
    build_name: String,
    game_version: Option<String>,
    path: String,
    size_bytes: u64,
    modified_at: Option<u64>,
    layout: String,
}

fn valid_title_id(value: &str) -> bool {
    (8..=16).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.@".contains(&b))
}

pub(super) fn path_is_within(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        let child = path.to_string_lossy().replace('/', "\\").to_lowercase();
        let parent = root.to_string_lossy().replace('/', "\\").to_lowercase();
        child == parent
            || child
                .strip_prefix(&parent)
                .is_some_and(|suffix| suffix.starts_with('\\') || parent.ends_with('\\'))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

fn validate_archive_relative_path(rel: &Path, title: &str) -> Result<String, String> {
    if rel.as_os_str().is_empty()
        || rel.to_string_lossy().contains('\\')
        || rel
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        || rel.components().next().map(|part| {
            part.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(title)
        }) != Some(true)
    {
        return Err("ZIP path is absolute, traversing, or outside the title folder".into());
    }
    Ok(rel.to_string_lossy().trim_end_matches('/').to_lowercase())
}

fn validate_archive_file_size(size: u64, compressed_size: u64) -> Result<(), String> {
    if size > 2 * 1024 * 1024 * 1024 {
        return Err("ZIP entry exceeds the 2 GiB per-file limit".into());
    }
    if (compressed_size == 0 && size > 0)
        || (compressed_size > 0 && size / compressed_size.max(1) > 1000)
    {
        return Err("ZIP compression ratio exceeds safety limit".into());
    }
    Ok(())
}

fn should_remove_backup(timestamp: i64, cutoff: i64, newest_verified: i64) -> bool {
    timestamp < cutoff && timestamp < newest_verified
}

fn backup_identity(
    build_id: i64,
    user_id: Option<String>,
    layout: String,
) -> (i64, Option<String>, String) {
    (build_id, user_id, layout)
}

#[cfg(test)]
fn safe_copy(from: &Path, to: &Path) -> Result<u64, String> {
    safe_copy_with_progress(from, to, |_, _| {})
}

fn safe_copy_with_progress(
    from: &Path,
    to: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<u64, String> {
    if to.exists() {
        return Err("Staging destination already exists; refusing to merge files".into());
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let total = directory_size(from);
    let result = (|| -> Result<u64, String> {
        let mut bytes = 0u64;
        for item in walkdir::WalkDir::new(from).follow_links(false) {
            let item = item.map_err(|e| format!("Could not inspect save entry: {e}"))?;
            let rel = item.path().strip_prefix(from).map_err(|e| e.to_string())?;
            if rel.as_os_str().is_empty() {
                continue;
            }
            let meta = fs::symlink_metadata(item.path()).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("Save contains a symbolic link; refusing unsafe copy".into());
            }
            let dest = to.join(rel);
            if meta.is_dir() {
                fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            } else if meta.is_file() {
                bytes = bytes.checked_add(meta.len()).ok_or("Save size overflow")?;
                if bytes > MAX_SAVE_BYTES {
                    return Err("Data exceeds the 8 GiB transfer safety limit".into());
                }
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::copy(item.path(), &dest).map_err(|e| e.to_string())?;
                fs::set_permissions(&dest, meta.permissions()).map_err(|e| e.to_string())?;
                if let Ok(modified) = meta.modified() {
                    let file = fs::OpenOptions::new()
                        .write(true)
                        .open(&dest)
                        .map_err(|e| e.to_string())?;
                    file.set_times(fs::FileTimes::new().set_modified(modified))
                        .map_err(|e| e.to_string())?;
                }
                progress(bytes, total);
            } else {
                return Err("Save contains an unsupported filesystem object".into());
            }
        }
        Ok(bytes)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(to);
    }
    result
}

fn tree_hash(path: &Path) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for item in walkdir::WalkDir::new(path)
        .follow_links(false)
        .sort_by_file_name()
    {
        let item = item.map_err(|e| e.to_string())?;
        if item.file_type().is_symlink() {
            return Err("Save contains a symbolic link".into());
        }
        if item.file_type().is_file() {
            let rel = item.path().strip_prefix(path).map_err(|e| e.to_string())?;
            hasher.update(rel.to_string_lossy().as_bytes());
            let mut file = fs::File::open(item.path()).map_err(|e| e.to_string())?;
            let mut buffer = [0u8; 65536];
            loop {
                let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn is_guest(path: &Path) -> Option<String> {
    let parts: Vec<_> = path
        .components()
        .map(|p| p.as_os_str().to_string_lossy().to_string())
        .collect();
    parts
        .windows(2)
        .find(|w| w[0] == "[users]")
        .map(|w| w[1].clone())
}

fn scan(app: &tauri::AppHandle, game_id: i64) -> Result<Vec<SaveEntry>, String> {
    let conn = super::db(app)?;
    let g = super::game(&conn, game_id)?;
    let title = g
        .title_id
        .as_deref()
        .filter(|s| valid_title_id(s))
        .ok_or("Game has no valid title ID")?;
    let builds = super::list_builds(app.clone())?;
    let repos = super::list_repos(app.clone())?;
    let mut entries = Vec::new();
    for b in builds {
        let repo = repos.iter().find(|r| r.id == b.repo_id);
        let root = PathBuf::from(&b.path).join("_SaveData");
        let mut candidates = Vec::new();
        let title_root = root.join(title);
        if let Ok(slots) = fs::read_dir(&title_root) {
            for slot in slots
                .flatten()
                .filter(|d| d.file_type().map(|t| t.is_dir()).unwrap_or(false))
            {
                let name = slot.file_name().to_string_lossy().to_string();
                if !name.starts_with("sce_sdmemory") && valid_component(&name) {
                    candidates.push((slot.path(), format!("title-slot:{name}")));
                }
            }
        }
        let users = root.join("[users]");
        if let Ok(dirs) = fs::read_dir(&users) {
            for user in dirs.flatten().filter(|d| {
                d.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && d.file_name().to_string_lossy().parse::<i32>().is_ok()
            }) {
                let user_root = user.path().join(title);
                if let Ok(slots) = fs::read_dir(&user_root) {
                    for slot in slots
                        .flatten()
                        .filter(|d| d.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    {
                        let name = slot.file_name().to_string_lossy().to_string();
                        if valid_component(&name) {
                            candidates.push((slot.path(), format!("guest-slot:{name}")));
                        }
                    }
                }
            }
        }
        // Memory saves are rooted at _SaveData/<TITLE>/sce_sdmemory[0..3]/<USER>.
        for slot in 0..4 {
            let name = if slot == 0 {
                "sce_sdmemory".to_string()
            } else {
                format!("sce_sdmemory{slot}")
            };
            let slot_root = root.join(title).join(&name);
            if let Ok(dirs) = fs::read_dir(&slot_root) {
                for user in dirs.flatten().filter(|d| {
                    d.file_type().map(|t| t.is_dir()).unwrap_or(false)
                        && d.file_name().to_string_lossy().parse::<i32>().is_ok()
                }) {
                    candidates.push((user.path(), format!("memory:{name}")));
                }
            }
        }
        for (path, layout) in candidates {
            let Ok(canonical) = fs::canonicalize(&path) else {
                continue;
            };
            let Ok(base) = fs::canonicalize(&root) else {
                continue;
            };
            if !path_is_within(&canonical, &base) || !canonical.is_dir() {
                continue;
            }
            let size = directory_size(&canonical);
            if size == 0 {
                continue;
            }
            let modified_at = fs::metadata(&canonical)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let relative = canonical.strip_prefix(&base).unwrap_or(&canonical);
            let user_id = is_guest(relative).or_else(|| {
                if layout.starts_with("memory:") {
                    relative
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                } else {
                    None
                }
            });
            let slot = layout
                .split_once(':')
                .map(|(_, name)| name.to_string())
                .unwrap_or_else(|| "unknown".into());
            let id = format!("{}:{}", b.id, canonical.display());
            entries.push(SaveEntry {
                id,
                title_id: title.to_string(),
                user_id,
                slot,
                build_id: b.id,
                build_name: format!(
                    "{} · {} · {}",
                    repo.map(|r| r.owner.as_str()).unwrap_or("Unknown"),
                    repo.map(|r| r.name.as_str()).unwrap_or("build"),
                    b.commit_sha.chars().take(8).collect::<String>()
                ),
                game_version: g.game_version.clone(),
                path: canonical.display().to_string(),
                size_bytes: size,
                modified_at,
                layout,
            });
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries.dedup_by(|a, b| a.path == b.path);
    Ok(entries)
}

#[tauri::command]
pub(super) fn list_saves(app: tauri::AppHandle, game_id: i64) -> Result<Vec<SaveEntry>, String> {
    scan(&app, game_id)
}

/// Keep a selected build's saves aligned with the last build that actually ran
/// this game. Existing target saves are backed up before they are replaced.
pub(super) fn sync_saves_from_last_played_build(
    app: &tauri::AppHandle,
    game_id: i64,
    target_build_id: i64,
) -> Result<usize, String> {
    let conn = super::db(app)?;
    let game = super::game(&conn, game_id)?;
    if !game.title_id.as_deref().is_some_and(valid_title_id) {
        return Ok(0);
    }
    let target_build = super::build(&conn, target_build_id)?;
    let target_root = fs::canonicalize(&target_build.path).map_err(|e| e.to_string())?;
    let entries = scan(app, game_id)?;
    let source_build_id = conn
        .query_row(
            "SELECT build_id FROM sessions WHERE game_id=? AND process_pid IS NOT NULL AND ended_at IS NOT NULL ORDER BY started_at DESC,id DESC LIMIT 1",
            [game_id],
            |row| row.get::<_, i64>(0),
        )
        .ok()
        .or_else(|| {
            entries
                .iter()
                .filter(|entry| entry.build_id != target_build_id)
                .max_by_key(|entry| entry.modified_at.unwrap_or(0))
                .map(|entry| entry.build_id)
        });
    let Some(source_build_id) = source_build_id.filter(|id| *id != target_build_id) else {
        return Ok(0);
    };
    let mut copied = 0;
    for entry in entries
        .iter()
        .filter(|entry| entry.build_id == source_build_id)
    {
        let destination = save_target(
            &target_root,
            &entry.title_id,
            &entry.layout,
            entry.user_id.as_deref(),
        )?;
        if destination.exists()
            && !entries.iter().any(|existing| {
                existing.build_id == target_build_id && Path::new(&existing.path) == destination
            })
        {
            if destination.is_dir() && directory_size(&destination) == 0 {
                fs::remove_dir(&destination).map_err(|e| e.to_string())?;
            } else {
                continue;
            }
        }
        if destination.exists() && tree_hash(&destination)? == tree_hash(Path::new(&entry.path))? {
            continue;
        }
        install_payload(
            app,
            game_id,
            target_build_id,
            &destination,
            Path::new(&entry.path),
            "before-auto-sync",
        )?;
        copied += 1;
    }
    Ok(copied)
}

pub(super) fn auto_backup_after_session(app: &tauri::AppHandle, game_id: i64) {
    let Ok(_lifecycle) = super::SESSION_LIFECYCLE_LOCK.lock() else {
        return;
    };
    let enabled = super::db(app)
        .ok()
        .and_then(|c| {
            c.query_row(
                "SELECT value FROM settings WHERE key='save_data_auto_backup'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
        })
        .is_some_and(|v| v == "true");
    if !enabled || assert_no_sessions(app, game_id).is_err() {
        return;
    }
    if let Ok(entries) = scan(app, game_id) {
        for entry in entries {
            let _ = backup_entry(app, game_id, &entry, Some("after-session"));
        }
    }
}

pub(super) fn backup_build_saves_locked(
    app: &tauri::AppHandle,
    build_id: i64,
) -> Result<(), String> {
    let conn = super::db(app)?;
    let mut statement = conn
        .prepare("SELECT id FROM games WHERE title_id IS NOT NULL")
        .map_err(|e| e.to_string())?;
    let ids = statement
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for game_id in ids {
        assert_no_sessions(app, game_id)?;
        for entry in scan(app, game_id)?
            .into_iter()
            .filter(|entry| entry.build_id == build_id)
        {
            backup_entry(app, game_id, &entry, Some("before-build-removal"))?;
        }
    }
    Ok(())
}

/// Keep emulator-owned save and trophy state when a repository build is replaced.
/// The previous installation is left untouched until the replacement copies verify.
pub(super) fn preserve_user_data_for_build_replacement(
    app: &tauri::AppHandle,
    build_id: i64,
    previous: &Path,
    replacement: &Path,
    managed_root: &Path,
) -> Result<(), String> {
    let root = fs::canonicalize(managed_root).map_err(|e| e.to_string())?;
    let old =
        fs::canonicalize(previous).map_err(|e| format!("Previous build is unavailable: {e}"))?;
    let new = fs::canonicalize(replacement)
        .map_err(|e| format!("Replacement build is unavailable: {e}"))?;
    if !path_is_within(&old, &root) || !path_is_within(&new, &root) || old == new {
        return Err("Build save preservation was refused because an installation is outside Studio's managed build folder".into());
    }

    // These verified backups provide recovery independently of the raw directory copy.
    backup_build_saves_locked(app, build_id)?;
    super::trophies::backup_build_trophies(app, build_id)?;

    for folder in ["_SaveData", "_Trophies", "_PipelineCache"] {
        preserve_build_data_folder(&old, &new, folder)?;
    }
    Ok(())
}

fn preserve_build_data_folder(old: &Path, new: &Path, folder: &str) -> Result<(), String> {
    let source = old.join(folder);
    let metadata = match fs::symlink_metadata(&source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{folder} is not a regular directory; refusing to replace the build"
        ));
    }
    let target = new.join(folder);
    if target.exists() {
        return Err(format!(
            "Replacement already contains {folder}; refusing to overwrite either build's data"
        ));
    }
    let staging = new.join(format!(".{folder}.preserve-{}", nonce()));
    safe_copy_with_progress(&source, &staging, |_, _| {})
        .map_err(|e| format!("Could not preserve {folder}: {e}"))?;
    if tree_hash(&source)? != tree_hash(&staging)? {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!(
            "Copied {folder} failed checksum verification; previous build remains intact"
        ));
    }
    if let Err(error) = fs::rename(&staging, &target) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("Could not install preserved {folder}: {error}"));
    }
    Ok(())
}

fn assert_no_sessions(app: &tauri::AppHandle, game_id: i64) -> Result<(), String> {
    let conn = super::db(app)?;
    let active: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE game_id=? AND ended_at IS NULL",
            [game_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if active != 0 {
        return Err("Close the emulator session before changing save data".into());
    }
    Ok(())
}

fn backup_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let paths = super::state_paths(app)?;
    let configured = super::db(app)?
        .query_row(
            "SELECT value FROM settings WHERE key='save_data_backup_directory'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok();
    let root = configured
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.data.join("save-backups"));
    fs::create_dir_all(&root).map_err(|e| format!("Could not create backup directory: {e}"))?;
    let canonical = fs::canonicalize(&root).map_err(|e| e.to_string())?;
    for build in super::list_builds(app.clone())? {
        if let Ok(build_path) = fs::canonicalize(build.path) {
            if path_is_within(&canonical, &build_path) {
                return Err(
                    "Backup directory must be outside every emulator build directory".into(),
                );
            }
        }
    }
    Ok(canonical)
}

#[tauri::command]
pub(super) fn set_save_backup_directory(
    app: tauri::AppHandle,
    path: String,
) -> Result<String, String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    let selected =
        fs::canonicalize(&path).map_err(|e| format!("Select an existing backup folder: {e}"))?;
    if !selected.is_dir() {
        return Err("Backup destination must be a directory".into());
    }
    for build in super::list_builds(app.clone())? {
        if let Ok(build_path) = fs::canonicalize(build.path) {
            if path_is_within(&selected, &build_path) {
                return Err(
                    "Backup directory must be outside every emulator build directory".into(),
                );
            }
        }
    }
    let conn = super::db(&app)?;
    conn.execute("INSERT INTO settings(key,value) VALUES('save_data_backup_directory',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[selected.to_string_lossy().as_ref()]).map_err(|e|e.to_string())?;
    Ok(selected.display().to_string())
}

#[tauri::command]
pub(super) fn clean_save_backups(
    app: tauri::AppHandle,
    game_id: i64,
    retention_days: u32,
) -> Result<u64, String> {
    if !(1..=3650).contains(&retention_days) {
        return Err("Retention must be between 1 and 3,650 days".into());
    }
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    assert_no_sessions(&app, game_id)?;
    let conn = super::db(&app)?;
    let game = super::game(&conn, game_id)?;
    let title = game
        .title_id
        .as_deref()
        .filter(|s| valid_title_id(s))
        .ok_or("Game has no valid title ID")?;
    let root = backup_root(&app)?.join(title);
    let Ok(items) = fs::read_dir(&root) else {
        return Ok(0);
    };
    let mut valid = Vec::<(PathBuf, Manifest, u64)>::new();
    for item in items.flatten() {
        if !item.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let path = item.path();
        let Ok(manifest) = fs::read(path.join("manifest.json"))
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<Manifest>(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };
        if manifest.title_id != title
            || tree_hash(&path.join("data")).ok().as_deref() != Some(&manifest.checksum_sha256)
            || manifest.includes_trophies != !manifest.trophy_files.is_empty()
            || !manifest.trophy_files.iter().all(|f| {
                super::trophies::valid_progress_filename(&f.filename)
                    && fs::read(path.join("trophies").join(&f.filename)).is_ok_and(|bytes| {
                        format!("{:x}", Sha256::digest(bytes)) == f.checksum_sha256
                    })
            })
        {
            continue;
        }
        let size = directory_size(&path);
        valid.push((path, manifest, size));
    }
    let mut latest = HashMap::<(i64, Option<String>, String), i64>::new();
    for (_, manifest, _) in &valid {
        let key = backup_identity(
            manifest.source_build_id,
            manifest.user_id.clone(),
            manifest.source_layout.clone(),
        );
        latest
            .entry(key)
            .and_modify(|timestamp| *timestamp = (*timestamp).max(manifest.timestamp))
            .or_insert(manifest.timestamp);
    }
    let cutoff = now().saturating_sub(i64::from(retention_days).saturating_mul(86400));
    let mut removed = 0u64;
    for (path, manifest, size) in valid {
        let key = backup_identity(
            manifest.source_build_id,
            manifest.user_id.clone(),
            manifest.source_layout.clone(),
        );
        if latest
            .get(&key)
            .is_some_and(|newest| should_remove_backup(manifest.timestamp, cutoff, *newest))
        {
            fs::remove_dir_all(path).map_err(|e| e.to_string())?;
            removed = removed.saturating_add(size);
        }
    }
    Ok(removed)
}

#[derive(Serialize, Deserialize)]
pub(super) struct TrophyBundleFile {
    pub(super) filename: String,
    pub(super) checksum_sha256: String,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    title_id: String,
    user_id: Option<String>,
    source_fork: String,
    source_build: String,
    source_build_id: i64,
    source_commit: String,
    game_version: Option<String>,
    source_layout: String,
    timestamp: i64,
    checksum_sha256: String,
    includes_trophies: bool,
    format_version: u32,
    #[serde(default)]
    label: String,
    #[serde(default)]
    trophy_files: Vec<TrophyBundleFile>,
    #[serde(default)]
    trophy_user_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ArchiveManifest {
    format_version: u32,
    title_id: String,
    user_id: Option<String>,
    layout: String,
    checksum_sha256: String,
    source_fork: String,
    source_commit: String,
    game_version: Option<String>,
}

fn save_target(
    build_root: &Path,
    title: &str,
    layout: &str,
    user: Option<&str>,
) -> Result<PathBuf, String> {
    if !valid_title_id(title) {
        return Err("Invalid save title ID".into());
    }
    let base = build_root.join("_SaveData");
    if let Some(slot) = layout.strip_prefix("title-slot:") {
        if !valid_component(slot) {
            return Err("Invalid title save slot".into());
        }
        Ok(base.join(title).join(slot))
    } else if let Some(slot) = layout.strip_prefix("guest-slot:") {
        let user = user
            .filter(|value| value.parse::<i32>().is_ok())
            .ok_or("Invalid guest user ID")?;
        if !valid_component(slot) {
            return Err("Invalid guest save slot".into());
        }
        Ok(base.join("[users]").join(user).join(title).join(slot))
    } else if let Some(slot) = layout.strip_prefix("memory:") {
        let user = user
            .filter(|value| value.parse::<i32>().is_ok())
            .ok_or("Invalid memory-save user ID")?;
        if !(slot == "sce_sdmemory" || (1..=3).any(|n| slot == format!("sce_sdmemory{n}"))) {
            return Err("Unsupported memory-save slot".into());
        }
        Ok(base.join(title).join(slot).join(user))
    } else {
        Err("Unsupported save layout".into())
    }
}

fn install_payload(
    app: &tauri::AppHandle,
    game_id: i64,
    build_id: i64,
    target: &Path,
    payload: &Path,
    label: &str,
) -> Result<(), String> {
    let conn = super::db(app)?;
    let build = super::build(&conn, build_id)?;
    let canonical_build = fs::canonicalize(&build.path).map_err(|e| e.to_string())?;
    if !path_is_within(target, &canonical_build) {
        return Err("Save destination escaped the selected build".into());
    }
    if target.exists() {
        let existing = scan(app, game_id)?
            .into_iter()
            .find(|entry| entry.build_id == build_id && Path::new(&entry.path) == target)
            .ok_or("Destination exists but is not a verified save")?;
        backup_entry(app, game_id, &existing, Some(label))?;
        replace_directory(payload, target, |completed, total| {
            emit_progress(app, game_id, label, "copying", completed, total)
        })
    } else {
        let parent = target.parent().ok_or("Save destination has no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let stage = parent.join(format!(".save-import-{}", nonce()));
        safe_copy_with_progress(payload, &stage, |completed, total| {
            emit_progress(app, game_id, label, "copying", completed, total)
        })?;
        fs::rename(stage, target).map_err(|e| e.to_string())
    }
}

fn verify_registered_import_source(
    app: &tauri::AppHandle,
    source: &Path,
    target_build_id: i64,
) -> Result<(), String> {
    let builds = super::list_builds(app.clone())?;
    let Some(source_build) = builds.into_iter().find(|build| {
        fs::canonicalize(PathBuf::from(&build.path).join("_SaveData"))
            .is_ok_and(|root| path_is_within(source, &root))
    }) else {
        return Ok(());
    };
    if source_build.id == target_build_id {
        return Ok(());
    }
    let conn = super::db(app)?;
    let target_build = super::build(&conn, target_build_id)?;
    let source_repo = super::repo(&conn, source_build.repo_id)?;
    let target_repo = super::repo(&conn, target_build.repo_id)?;
    if source_build.commit_sha != target_build.commit_sha
        || source_repo.owner != target_repo.owner
        || source_repo.name != target_repo.name
    {
        return Err("Directory belongs to a different registered build; cross-build save migration cannot be verified".into());
    }
    Ok(())
}

fn backup_entry(
    app: &tauri::AppHandle,
    game_id: i64,
    entry: &SaveEntry,
    label: Option<&str>,
) -> Result<String, String> {
    backup_entry_internal(app, game_id, entry, label, false, None)
}

fn backup_entry_internal(
    app: &tauri::AppHandle,
    game_id: i64,
    entry: &SaveEntry,
    label: Option<&str>,
    include_trophies: bool,
    trophy_user_id: Option<&str>,
) -> Result<String, String> {
    let label = label
        .unwrap_or("backup")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
        .take(64)
        .collect::<String>();
    let root = backup_root(app)?;
    let hash = tree_hash(Path::new(&entry.path))?;
    let parent = root.join(&entry.title_id);
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    for item in fs::read_dir(&parent).map_err(|e| e.to_string())?.flatten() {
        let manifest_path = item.path().join("manifest.json");
        if let Ok(data) = fs::read(&manifest_path) {
            if let Ok(manifest) = serde_json::from_slice::<Manifest>(&data) {
                if !include_trophies
                    && !manifest.includes_trophies
                    && manifest.checksum_sha256 == hash
                    && manifest.title_id == entry.title_id
                    && manifest.user_id == entry.user_id
                    && manifest.source_layout == entry.layout
                    && manifest.source_build_id == entry.build_id
                    && manifest.label == label
                    && tree_hash(&item.path().join("data")).ok().as_deref() == Some(&hash)
                {
                    return Ok(item.path().display().to_string());
                }
            }
        }
    }
    let folder = format!("{}-{}-{}", nonce(), label.as_str(), &hash[..12]);
    let destination = parent.join(folder);
    let stage = parent.join(format!(".staging-{}", nonce()));
    let staged = (|| -> Result<(), String> {
        emit_progress(app, game_id, "backup", "copying", 0, entry.size_bytes);
        safe_copy_with_progress(
            Path::new(&entry.path),
            &stage.join("data"),
            |completed, total| emit_progress(app, game_id, "backup", "copying", completed, total),
        )?;
        let trophy_files = if include_trophies {
            super::trophies::copy_for_save_backup(
                app,
                game_id,
                entry.build_id,
                &entry.title_id,
                trophy_user_id.or(entry.user_id.as_deref()),
                &stage.join("trophies"),
            )?
        } else {
            Vec::new()
        };
        let conn = super::db(app)?;
        let build = super::build(&conn, entry.build_id)?;
        let repo = super::repo(&conn, build.repo_id).ok();
        let manifest = Manifest {
            title_id: entry.title_id.clone(),
            user_id: entry.user_id.clone(),
            source_fork: repo
                .map(|r| format!("{}/{}", r.owner, r.name))
                .unwrap_or_default(),
            source_build: entry.build_name.clone(),
            source_build_id: entry.build_id,
            source_commit: build.commit_sha,
            game_version: entry.game_version.clone(),
            source_layout: entry.layout.clone(),
            timestamp: now(),
            checksum_sha256: hash.clone(),
            includes_trophies: !trophy_files.is_empty(),
            format_version: 1,
            label,
            trophy_files,
            trophy_user_id: include_trophies
                .then(|| {
                    trophy_user_id
                        .or(entry.user_id.as_deref())
                        .map(str::to_owned)
                })
                .flatten(),
        };
        fs::write(
            stage.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(&stage, &destination).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = staged {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    let _ = game_id;
    Ok(destination.display().to_string())
}

#[derive(Serialize)]
pub(super) struct BackupRecord {
    path: String,
    title_id: String,
    user_id: Option<String>,
    trophy_user_id: Option<String>,
    source_build: String,
    source_build_id: i64,
    source_fork: String,
    timestamp: i64,
    checksum_sha256: String,
    label: String,
    includes_trophies: bool,
    trophy_filenames: Vec<String>,
}

#[tauri::command]
pub(super) fn list_save_backups(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<Vec<BackupRecord>, String> {
    let conn = super::db(&app)?;
    let g = super::game(&conn, game_id)?;
    let title = g
        .title_id
        .as_deref()
        .filter(|s| valid_title_id(s))
        .ok_or("Game has no valid title ID")?;
    let root = backup_root(&app)?.join(title);
    let mut records = Vec::new();
    let Ok(items) = fs::read_dir(root) else {
        return Ok(records);
    };
    for item in items.flatten() {
        if !item.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let path = item.path();
        let Ok(manifest) = fs::read(path.join("manifest.json"))
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<Manifest>(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };
        if manifest.title_id == title
            && manifest.includes_trophies == !manifest.trophy_files.is_empty()
            && tree_hash(&path.join("data")).ok().as_deref() == Some(&manifest.checksum_sha256)
            && manifest.trophy_files.iter().all(|f| {
                super::trophies::valid_progress_filename(&f.filename)
                    && fs::read(path.join("trophies").join(&f.filename)).is_ok_and(|bytes| {
                        format!("{:x}", Sha256::digest(bytes)) == f.checksum_sha256
                    })
            })
        {
            records.push(BackupRecord {
                path: path.display().to_string(),
                title_id: manifest.title_id,
                user_id: manifest.user_id,
                trophy_user_id: manifest.trophy_user_id,
                source_build: manifest.source_build,
                source_build_id: manifest.source_build_id,
                source_fork: manifest.source_fork,
                timestamp: manifest.timestamp,
                checksum_sha256: manifest.checksum_sha256,
                label: manifest.label,
                includes_trophies: manifest.includes_trophies,
                trophy_filenames: manifest
                    .trophy_files
                    .iter()
                    .map(|f| f.filename.clone())
                    .collect(),
            });
        }
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.timestamp));
    Ok(records)
}

#[tauri::command]
pub(super) fn backup_save(
    app: tauri::AppHandle,
    game_id: i64,
    save_path: String,
    label: Option<String>,
) -> Result<String, String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    emit_progress(&app, game_id, "backup", "starting", 0, 1);
    assert_no_sessions(&app, game_id)?;
    let entry = scan(&app, game_id)?
        .into_iter()
        .find(|e| e.path == save_path)
        .ok_or("Save path is no longer a verified save")?;
    let result = backup_entry(&app, game_id, &entry, label.as_deref());
    emit_progress(
        &app,
        game_id,
        "backup",
        if result.is_ok() { "complete" } else { "failed" },
        1,
        1,
    );
    result
}

#[tauri::command]
pub(super) fn backup_save_with_trophies(
    app: tauri::AppHandle,
    game_id: i64,
    save_path: String,
    label: Option<String>,
    trophy_user_id: Option<String>,
) -> Result<String, String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    emit_progress(&app, game_id, "combined-backup", "starting", 0, 1);
    assert_no_sessions(&app, game_id)?;
    let entry = scan(&app, game_id)?
        .into_iter()
        .find(|e| e.path == save_path)
        .ok_or("Save path is no longer verified")?;
    let user = trophy_user_id.as_deref();
    if entry
        .user_id
        .as_deref()
        .is_some_and(|save_user| Some(save_user) != user)
    {
        return Err("Selected trophy user does not match the save user".into());
    }
    let result = backup_entry_internal(&app, game_id, &entry, label.as_deref(), true, user);
    emit_progress(
        &app,
        game_id,
        "combined-backup",
        if result.is_ok() { "complete" } else { "failed" },
        1,
        1,
    );
    result
}

#[tauri::command]
pub(super) fn restore_save(
    app: tauri::AppHandle,
    game_id: i64,
    backup_path: String,
) -> Result<(), String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    emit_progress(&app, game_id, "restore", "validating", 0, 1);
    assert_no_sessions(&app, game_id)?;
    let root = backup_root(&app)?;
    let backup = fs::canonicalize(&backup_path).map_err(|e| e.to_string())?;
    let canonical_root = fs::canonicalize(&root).map_err(|_| "No save backups exist")?;
    if !path_is_within(&backup, &canonical_root) {
        return Err("Backup is outside Studio's managed backup folder".into());
    }
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(backup.join("manifest.json")).map_err(|e| e.to_string())?)
            .map_err(|_| "Backup manifest is invalid")?;
    if tree_hash(&backup.join("data"))? != manifest.checksum_sha256 {
        return Err("Backup checksum verification failed".into());
    }
    let conn = super::db(&app)?;
    let g = super::game(&conn, game_id)?;
    if g.title_id.as_deref() != Some(manifest.title_id.as_str()) {
        return Err("Backup title ID does not match this game".into());
    }
    let b = super::build(&conn, manifest.source_build_id)?;
    let canonical_build = fs::canonicalize(&b.path).map_err(|e| e.to_string())?;
    let target = save_target(
        &canonical_build,
        &manifest.title_id,
        &manifest.source_layout,
        manifest.user_id.as_deref(),
    )?;
    if manifest.includes_trophies {
        super::trophies::validate_save_bundle_trophies(
            &backup.join("trophies"),
            manifest
                .trophy_user_id
                .as_deref()
                .or(manifest.user_id.as_deref()),
            &manifest.trophy_files,
        )?;
    } else if !manifest.trophy_files.is_empty() {
        return Err("Backup manifest has trophy files but does not declare them".into());
    }
    let result = install_payload(
        &app,
        game_id,
        manifest.source_build_id,
        &target,
        &backup.join("data"),
        "pre-restore",
    );
    let result = result.and_then(|()| {
        if manifest.includes_trophies {
            super::trophies::restore_save_bundle_trophies(
                &app,
                game_id,
                manifest.source_build_id,
                &manifest.title_id,
                manifest
                    .trophy_user_id
                    .as_deref()
                    .or(manifest.user_id.as_deref()),
                &backup.join("trophies"),
                &manifest.trophy_files,
            )?;
        } else if !manifest.trophy_files.is_empty() {
            return Err("Backup manifest has trophy files but does not declare them".into());
        }
        Ok(())
    });
    emit_progress(
        &app,
        game_id,
        "restore",
        if result.is_ok() { "complete" } else { "failed" },
        1,
        1,
    );
    result
}

fn replace_directory(
    source: &Path,
    target: &Path,
    progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    let parent = target.parent().ok_or("Save path has no parent")?;
    let stage = parent.join(format!(".save-stage-{}", nonce()));
    let old = parent.join(format!(".save-old-{}", nonce()));
    safe_copy_with_progress(source, &stage, progress)?;
    fs::rename(target, &old).map_err(|e| {
        let _ = fs::remove_dir_all(&stage);
        e.to_string()
    })?;
    if let Err(e) = fs::rename(&stage, target) {
        let rollback = fs::rename(&old, target);
        let _ = fs::remove_dir_all(&stage);
        return Err(format!(
            "Install failed: {e}; rollback {}",
            if rollback.is_ok() {
                "succeeded"
            } else {
                "FAILED; original remains at backup path"
            }
        ));
    }
    fs::remove_dir_all(old)
        .map_err(|e| format!("Save installed, but old staging cleanup failed: {e}"))?;
    Ok(())
}

#[tauri::command]
pub(super) fn import_save_directory(
    app: tauri::AppHandle,
    game_id: i64,
    source_path: String,
    build_id: i64,
) -> Result<String, String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    emit_progress(&app, game_id, "import-folder", "validating", 0, 1);
    let result = import_save_directory_impl(app.clone(), game_id, source_path, build_id);
    emit_progress(
        &app,
        game_id,
        "import-folder",
        if result.is_ok() { "complete" } else { "failed" },
        1,
        1,
    );
    result
}

fn import_save_directory_impl(
    app: tauri::AppHandle,
    game_id: i64,
    source_path: String,
    build_id: i64,
) -> Result<String, String> {
    assert_no_sessions(&app, game_id)?;
    let conn = super::db(&app)?;
    let g = super::game(&conn, game_id)?;
    let title = g
        .title_id
        .as_deref()
        .filter(|s| valid_title_id(s))
        .ok_or("Game has no valid title ID")?;
    let source = fs::canonicalize(&source_path).map_err(|e| e.to_string())?;
    if !source.is_dir()
        || source
            .file_name()
            .map(|s| s.to_string_lossy().to_uppercase())
            != Some(title.to_uppercase())
    {
        return Err("Select the save folder whose name matches this game's title ID".into());
    }
    let b = super::build(&conn, build_id)?;
    verify_registered_import_source(&app, &source, build_id)?;
    let target = PathBuf::from(b.path).join("_SaveData").join(title);
    if target.exists() {
        let entries = scan(&app, game_id)?;
        let existing: Vec<_> = entries
            .into_iter()
            .filter(|e| e.build_id == build_id && Path::new(&e.path).starts_with(&target))
            .collect();
        if existing.is_empty() {
            return Err(
                "Destination exists but contains no fully verified saves; refusing overwrite"
                    .into(),
            );
        }
        for item in fs::read_dir(&target).map_err(|e| e.to_string())?.flatten() {
            if !item.file_type().map_err(|e| e.to_string())?.is_dir() {
                return Err(
                    "Destination contains unrecognized root files; refusing overwrite".into(),
                );
            }
            let name = item.file_name().to_string_lossy().to_string();
            if name.starts_with("sce_sdmemory") {
                let supported_slot = name == "sce_sdmemory"
                    || (1..=3).any(|slot| name == format!("sce_sdmemory{slot}"));
                if !supported_slot {
                    return Err("Destination contains an unsupported memory-save slot".into());
                }
                for user in fs::read_dir(item.path())
                    .map_err(|e| e.to_string())?
                    .flatten()
                {
                    let user_id = user.file_name().to_string_lossy().to_string();
                    if user_id.parse::<i32>().is_err()
                        || !existing.iter().any(|e| {
                            Path::new(&e.path).starts_with(item.path())
                                && Path::new(&e.path)
                                    .file_name()
                                    .is_some_and(|s| s.to_string_lossy() == user_id)
                        })
                    {
                        return Err("Destination contains an unverified memory-save user".into());
                    }
                }
            } else if !valid_component(&name)
                || !existing.iter().any(|e| {
                    Path::new(&e.path)
                        .file_name()
                        .is_some_and(|s| s.to_string_lossy() == name)
                })
            {
                return Err(
                    "Destination contains an unrecognized save slot; refusing overwrite".into(),
                );
            }
        }
        for item in existing {
            backup_entry(&app, game_id, &item, Some("pre-import"))?;
        }
        replace_directory(&source, &target, |completed, total| {
            emit_progress(&app, game_id, "import-folder", "copying", completed, total)
        })?;
    } else {
        let parent = target.parent().ok_or("Invalid destination")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let stage = parent.join(format!(".save-stage-{}", nonce()));
        safe_copy_with_progress(&source, &stage, |completed, total| {
            emit_progress(&app, game_id, "import-folder", "copying", completed, total)
        })?;
        fs::rename(stage, &target).map_err(|e| e.to_string())?;
    }
    Ok(target.display().to_string())
}

#[tauri::command]
pub(super) fn import_save_archive(
    app: tauri::AppHandle,
    game_id: i64,
    archive_path: String,
    build_id: i64,
) -> Result<String, String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    emit_progress(&app, game_id, "import-zip", "validating", 0, 1);
    assert_no_sessions(&app, game_id)?;
    let conn = super::db(&app)?;
    let g = super::game(&conn, game_id)?;
    let title = g
        .title_id
        .as_deref()
        .filter(|s| valid_title_id(s))
        .ok_or("Game has no valid title ID")?;
    let file = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid ZIP archive: {e}"))?;
    if archive.is_empty() || archive.len() > 20_000 {
        return Err("ZIP must contain between 1 and 20,000 entries".into());
    }
    let staging_root = super::state_paths(&app)?
        .data
        .join("save-import-staging")
        .join(nonce());
    fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let archive_entries = archive.len() as u64;
    let extraction = (|| -> Result<PathBuf, String> {
        for i in 0..archive_entries as usize {
            let mut item = archive
                .by_index(i)
                .map_err(|e| format!("Invalid ZIP entry: {e}"))?;
            let rel = item
                .enclosed_name()
                .ok_or("ZIP contains an absolute or traversing path")?
                .to_path_buf();
            if rel.as_os_str().is_empty() {
                return Err("ZIP contains an empty entry name".into());
            }
            let key = validate_archive_relative_path(&rel, title)?;
            if !seen.insert(key) {
                return Err("ZIP contains duplicate paths".into());
            }
            if item
                .unix_mode()
                .map(|m| m & 0o170000 == 0o120000)
                .unwrap_or(false)
            {
                return Err("ZIP contains a symbolic link".into());
            }
            if !item.is_dir() {
                validate_archive_file_size(item.size(), item.compressed_size())?;
                total = total
                    .checked_add(item.size())
                    .ok_or("ZIP uncompressed size overflow")?;
                if total > MAX_SAVE_BYTES {
                    return Err("ZIP exceeds the save archive size limit".into());
                }
            }
            let destination = staging_root.join(&rel);
            if !path_is_within(&destination, &staging_root) {
                return Err("ZIP path escapes staging directory".into());
            }
            if item.is_dir() {
                fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut output = fs::File::create(&destination).map_err(|e| e.to_string())?;
                std::io::copy(&mut item, &mut output).map_err(|e| e.to_string())?;
                output.sync_all().map_err(|e| e.to_string())?;
            }
            if i % 32 == 0 || i + 1 == archive_entries as usize {
                emit_progress(
                    &app,
                    game_id,
                    "import-zip",
                    "extracting",
                    (i + 1) as u64,
                    archive_entries,
                );
            }
        }
        let folder = staging_root.join(title);
        if !folder.is_dir() {
            return Err("ZIP must contain a top-level folder matching the game's title ID".into());
        }
        Ok(folder)
    })();
    let result = match extraction {
        Ok(folder) => (|| -> Result<String, String> {
            let metadata: ArchiveManifest = serde_json::from_slice(
                &fs::read(folder.join(".kytyps5-save.json"))
                    .map_err(|_| "ZIP has no KytyPS5 save manifest")?,
            )
            .map_err(|_| "ZIP save manifest is invalid")?;
            if metadata.format_version != 1 || metadata.title_id != title {
                return Err("ZIP save manifest does not match this title".into());
            }
            let payload = folder.join("payload");
            if !payload.is_dir()
                || directory_size(&payload) == 0
                || tree_hash(&payload)? != metadata.checksum_sha256
            {
                return Err("ZIP save payload checksum failed".into());
            }
            let conn = super::db(&app)?;
            let build = super::build(&conn, build_id)?;
            let repo = super::repo(&conn, build.repo_id)?;
            let source_fork = format!("{}/{}", repo.owner, repo.name);
            let source_version = g.game_version.as_deref();
            if build.commit_sha != metadata.source_commit
                || source_fork != metadata.source_fork
                || source_version != metadata.game_version.as_deref()
            {
                return Err("ZIP source fork, commit, or known game version does not match the destination build; compatible migration could not be verified".into());
            }
            let build_root = fs::canonicalize(&build.path).map_err(|e| e.to_string())?;
            let target = save_target(
                &build_root,
                title,
                &metadata.layout,
                metadata.user_id.as_deref(),
            )?;
            install_payload(&app, game_id, build_id, &target, &payload, "pre-import")?;
            Ok(target.display().to_string())
        })(),
        Err(e) => Err(e),
    };
    let cleanup = fs::remove_dir_all(&staging_root);
    match (result, cleanup) {
        (Ok(path), Ok(())) => {
            emit_progress(&app, game_id, "import-zip", "complete", 1, 1);
            Ok(path)
        }
        (Ok(_), Err(error)) => Err(format!(
            "Save imported, but staging cleanup failed: {error}"
        )),
        (Err(error), _) => {
            emit_progress(&app, game_id, "import-zip", "failed", 0, 1);
            Err(error)
        }
    }
}

#[tauri::command]
pub(super) fn export_save_archive(
    app: tauri::AppHandle,
    game_id: i64,
    save_path: String,
    destination: String,
) -> Result<String, String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    assert_no_sessions(&app, game_id)?;
    let entry = scan(&app, game_id)?
        .into_iter()
        .find(|e| e.path == save_path)
        .ok_or("Save path is no longer verified")?;
    emit_progress(&app, game_id, "export-zip", "writing", 0, entry.size_bytes);
    if entry.size_bytes > MAX_SAVE_BYTES {
        return Err("Save exceeds the 8 GiB export limit".into());
    }
    let output =
        fs::File::create(&destination).map_err(|e| format!("Could not create ZIP: {e}"))?;
    let mut zip = zip::ZipWriter::new(output);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let root = Path::new(&entry.path);
    let conn = super::db(&app)?;
    let build = super::build(&conn, entry.build_id)?;
    let repo = super::repo(&conn, build.repo_id)?;
    let game = super::game(&conn, game_id)?;
    let metadata = ArchiveManifest {
        format_version: 1,
        title_id: entry.title_id.clone(),
        user_id: entry.user_id.clone(),
        layout: entry.layout.clone(),
        checksum_sha256: tree_hash(root)?,
        source_fork: format!("{}/{}", repo.owner, repo.name),
        source_commit: build.commit_sha,
        game_version: game.game_version,
    };
    let manifest_name = format!("{}/.kytyps5-save.json", entry.title_id);
    zip.start_file(manifest_name, options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut completed = 0u64;
    for item in walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
    {
        let item = item.map_err(|e| e.to_string())?;
        if item.file_type().is_symlink() {
            return Err("Save contains a symbolic link; refusing export".into());
        }
        if item.path() == root {
            continue;
        }
        let relative = item.path().strip_prefix(root).map_err(|e| e.to_string())?;
        let name = Path::new(&entry.title_id)
            .join("payload")
            .join(relative)
            .to_string_lossy()
            .replace('\\', "/");
        if item.file_type().is_dir() {
            zip.add_directory(format!("{name}/"), options)
                .map_err(|e| e.to_string())?;
        } else {
            let file_size = item.metadata().map_err(|e| e.to_string())?.len();
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            let mut file = fs::File::open(item.path()).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut zip).map_err(|e| e.to_string())?;
            completed = completed.saturating_add(file_size);
            emit_progress(
                &app,
                game_id,
                "export-zip",
                "writing",
                completed,
                entry.size_bytes,
            );
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    emit_progress(
        &app,
        game_id,
        "export-zip",
        "complete",
        entry.size_bytes,
        entry.size_bytes,
    );
    Ok(destination)
}

#[tauri::command]
pub(super) fn delete_save(
    app: tauri::AppHandle,
    game_id: i64,
    save_path: String,
) -> Result<(), String> {
    let _lifecycle = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    emit_progress(&app, game_id, "delete", "backing-up", 0, 1);
    assert_no_sessions(&app, game_id)?;
    let entry = scan(&app, game_id)?
        .into_iter()
        .find(|e| e.path == save_path)
        .ok_or("Save path is no longer verified")?;
    backup_entry(&app, game_id, &entry, Some("pre-delete"))?;
    let result = fs::remove_dir_all(entry.path).map_err(|e| e.to_string());
    emit_progress(
        &app,
        game_id,
        "delete",
        if result.is_ok() { "complete" } else { "failed" },
        1,
        1,
    );
    result
}

#[tauri::command]
pub(super) fn open_save_folder(path: String) -> Result<(), String> {
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("explorer.exe");
        c.arg(&path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(&path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(&path);
        c
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open save folder: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_ids_and_upstream_path_components_are_validated() {
        assert!(valid_title_id("PPSA01234"));
        assert!(!valid_title_id("../TITLE"));
        assert!(!valid_title_id("TITLE/ID"));
        assert!(valid_component("sce_sys"));
        assert!(!valid_component(".."));
        assert!(!valid_component("user/name"));
    }

    #[test]
    fn layout_targets_keep_slots_and_users_separate() {
        let build = Path::new("/opt/kyty/build");
        assert_eq!(
            save_target(build, "PPSA01234", "title-slot:slot-a", None).unwrap(),
            PathBuf::from("/opt/kyty/build/_SaveData/PPSA01234/slot-a")
        );
        assert_ne!(
            save_target(build, "PPSA01234", "title-slot:slot-a", None).unwrap(),
            save_target(build, "CUSA00678", "title-slot:slot-a", None).unwrap()
        );
        assert_eq!(
            save_target(build, "PPSA01234", "guest-slot:slot-a", Some("42")).unwrap(),
            PathBuf::from("/opt/kyty/build/_SaveData/[users]/42/PPSA01234/slot-a")
        );
        assert_ne!(
            save_target(build, "PPSA01234", "memory:sce_sdmemory1", Some("42")).unwrap(),
            save_target(build, "PPSA01234", "memory:sce_sdmemory1", Some("43")).unwrap()
        );
        assert!(save_target(build, "PPSA01234", "guest-slot:slot-a", Some("x")).is_err());
    }

    #[test]
    fn zip_paths_reject_traversal_and_duplicate_names_are_case_folded() {
        assert!(
            validate_archive_relative_path(Path::new("PPSA01234/payload/file"), "PPSA01234")
                .is_ok()
        );
        assert!(
            validate_archive_relative_path(Path::new("PPSA01234/../outside"), "PPSA01234").is_err()
        );
        assert!(validate_archive_relative_path(Path::new("OTHER/file"), "PPSA01234").is_err());
        let first =
            validate_archive_relative_path(Path::new("PPSA01234/Payload/File"), "PPSA01234")
                .unwrap();
        let second =
            validate_archive_relative_path(Path::new("PPSA01234/payload/file"), "PPSA01234")
                .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn retention_keeps_recent_and_last_verified_restore_point() {
        assert!(should_remove_backup(100, 200, 300));
        assert!(!should_remove_backup(100, 200, 100));
        assert!(!should_remove_backup(250, 200, 300));
    }

    #[test]
    fn combined_manifest_round_trip_keeps_explicit_trophy_files() {
        let manifest = Manifest {
            title_id: "CUSA00001".into(),
            user_id: Some("1000".into()),
            source_fork: "owner/repo".into(),
            source_build: "build".into(),
            source_build_id: 7,
            source_commit: "abc123".into(),
            game_version: Some("1.00".into()),
            source_layout: "guest-slot:slot0".into(),
            timestamp: 123,
            checksum_sha256: "savehash".into(),
            includes_trophies: true,
            format_version: 1,
            label: "manual".into(),
            trophy_user_id: Some("1000".into()),
            trophy_files: vec![TrophyBundleFile {
                filename: "trophies_1000_2.json".into(),
                checksum_sha256: "trophyhash".into(),
            }],
        };
        let encoded = serde_json::to_vec(&manifest).unwrap();
        let decoded: Manifest = serde_json::from_slice(&encoded).unwrap();
        assert!(decoded.includes_trophies);
        assert_eq!(decoded.trophy_files.len(), 1);
        assert_eq!(decoded.trophy_user_id.as_deref(), Some("1000"));
        assert_eq!(decoded.trophy_files[0].filename, "trophies_1000_2.json");
        assert_eq!(decoded.user_id.as_deref(), Some("1000"));
    }

    #[test]
    fn backup_identity_separates_build_users_and_slots() {
        let default = backup_identity(1, None, "title-slot:slot-a".into());
        assert_ne!(
            default,
            backup_identity(2, None, "title-slot:slot-a".into())
        );
        assert_ne!(
            default,
            backup_identity(1, Some("42".into()), "title-slot:slot-a".into())
        );
        assert_ne!(
            default,
            backup_identity(1, None, "title-slot:slot-b".into())
        );
    }

    #[test]
    fn archive_size_validation_rejects_empty_compression_and_bombs() {
        assert!(validate_archive_file_size(10, 0).is_err());
        assert!(validate_archive_file_size(1001, 1).is_err());
        assert!(
            validate_archive_file_size(3 * 1024 * 1024 * 1024, 2 * 1024 * 1024 * 1024).is_err()
        );
        assert!(validate_archive_file_size(1024, 2).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_containment_is_case_insensitive_and_boundary_aware() {
        assert!(path_is_within(
            Path::new(r"C:\Emulator\Build\_SaveData"),
            Path::new(r"c:\emulator\build")
        ));
        assert!(!path_is_within(
            Path::new(r"C:\Emulator\Builder"),
            Path::new(r"C:\Emulator\Build")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn linux_path_containment_remains_case_sensitive() {
        assert!(path_is_within(
            Path::new("/home/user/build/data"),
            Path::new("/home/user/build")
        ));
        assert!(!path_is_within(
            Path::new("/home/user/Build/data"),
            Path::new("/home/user/build")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn directory_copy_rejects_symbolic_link_escape() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("save-copy-{}", nonce()));
        let source = base.join("source");
        let target = base.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("save.bin"), b"save").unwrap();
        symlink("/etc/passwd", source.join("escape")).unwrap();
        assert!(safe_copy(&source, &target).is_err());
        assert!(!target.exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn directory_copy_handles_missing_source_without_creating_staging_data() {
        let base = std::env::temp_dir().join(format!("save-missing-{}", nonce()));
        let source = base.join("missing");
        let target = base.join("target");
        assert!(safe_copy(&source, &target).is_err());
        assert!(!target.exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn build_replacement_preserves_save_and_trophy_trees_without_overwriting() {
        let base = std::env::temp_dir().join(format!("build-replace-{}", nonce()));
        let old = base.join("old");
        let new = base.join("new");
        fs::create_dir_all(old.join("_SaveData/title/slot")).unwrap();
        fs::create_dir_all(old.join("_Trophies/title")).unwrap();
        fs::create_dir_all(old.join("_PipelineCache/shaders")).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("_SaveData/title/slot/USR-DATA"), b"save bytes").unwrap();
        fs::write(
            old.join("_Trophies/title/trophies_1_2.json"),
            b"trophy bytes",
        )
        .unwrap();
        fs::write(
            old.join("_PipelineCache/shaders/pipeline.bin"),
            b"cache bytes",
        )
        .unwrap();

        preserve_build_data_folder(&old, &new, "_SaveData").unwrap();
        preserve_build_data_folder(&old, &new, "_Trophies").unwrap();
        preserve_build_data_folder(&old, &new, "_PipelineCache").unwrap();
        assert_eq!(
            tree_hash(&old.join("_SaveData")).unwrap(),
            tree_hash(&new.join("_SaveData")).unwrap()
        );
        assert_eq!(
            tree_hash(&old.join("_Trophies")).unwrap(),
            tree_hash(&new.join("_Trophies")).unwrap()
        );
        assert_eq!(
            tree_hash(&old.join("_PipelineCache")).unwrap(),
            tree_hash(&new.join("_PipelineCache")).unwrap()
        );
        assert!(preserve_build_data_folder(&old, &new, "_SaveData").is_err());
        assert_eq!(
            fs::read(old.join("_SaveData/title/slot/USR-DATA")).unwrap(),
            b"save bytes"
        );
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn failed_staged_replacement_keeps_original_save() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("save-rollback-{}", nonce()));
        let source = base.join("incoming");
        let target = base.join("live");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("save.bin"), b"new").unwrap();
        fs::write(target.join("save.bin"), b"original").unwrap();
        symlink("/etc/passwd", source.join("escape")).unwrap();
        assert!(replace_directory(&source, &target, |_, _| {}).is_err());
        assert_eq!(fs::read(target.join("save.bin")).unwrap(), b"original");
        let _ = fs::remove_dir_all(base);
    }
}
