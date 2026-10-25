//! Local trophy progress preservation. Package definitions are read-only game content;
//! this module only copies the emulator's per-user/per-service earned-state JSON.
use super::*;
use serde::Serialize;

const MAX_TROPHY_FILE: u64 = 1024 * 1024;

#[derive(Clone, Serialize)]
pub(super) struct TrophyProgress {
    pub(super) id: String,
    pub(super) title_id: String,
    pub(super) build_id: i64,
    pub(super) build_name: String,
    pub(super) build_path: String,
    pub(super) user_id: String,
    pub(super) service_label: String,
    pub(super) path: String,
    pub(super) read_status: String,
    pub(super) earned: Option<usize>,
    pub(super) earned_ids: Vec<String>,
    pub(super) error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct TrophyManifest {
    format_version: u32,
    title_id: String,
    user_id: String,
    service_label: String,
    source_build_id: i64,
    source_build: String,
    source_path: String,
    timestamp: i64,
    sha256: String,
    filename: String,
}

fn parse_filename(name: &str) -> Option<(String, String)> {
    let stem = name.strip_prefix("trophies_")?.strip_suffix(".json")?;
    let (user, service) = stem.split_once('_')?;
    if user.parse::<u32>().is_err() || service.parse::<u32>().is_err() {
        return None;
    }
    Some((user.to_string(), service.to_string()))
}
pub(super) fn valid_progress_filename(name: &str) -> bool {
    parse_filename(name).is_some()
}

fn read_progress(path: &Path) -> Result<(Vec<String>, u64), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_TROPHY_FILE {
        return Err("Trophy progress file is not a regular file or exceeds 1 MiB".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid trophy JSON: {e}"))?;
    let values = value
        .get("unlockedTrophies")
        .and_then(|v| v.as_array())
        .ok_or("Trophy JSON has no unlockedTrophies array")?;
    let mut ids = Vec::new();
    for item in values {
        let id = item
            .as_u64()
            .map(|n| n.to_string())
            .or_else(|| {
                item.as_str()
                    .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                    .map(str::to_owned)
            })
            .ok_or("Trophy JSON contains an invalid trophy ID")?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok((ids, meta.len()))
}

fn valid_title(s: &str) -> bool {
    (8..=16).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn prepare_target(build_path: &Path, title: &str, filename: &str) -> Result<PathBuf, String> {
    if parse_filename(filename).is_none() {
        return Err("Invalid trophy target filename".into());
    }
    let root = fs::canonicalize(build_path).map_err(|e| e.to_string())?;
    let target = root.join("_Trophies").join(title).join(filename);
    let relative = target
        .strip_prefix(&root)
        .map_err(|_| "Trophy target escaped its build")?;
    let mut cursor = root.clone();
    for component in relative.components().take(2) {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err("Trophy target contains a symlink or non-folder component".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&cursor).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        }
        if !super::save_data::path_is_within(
            &fs::canonicalize(&cursor).map_err(|e| e.to_string())?,
            &root,
        ) {
            return Err("Trophy target escaped its registered build".into());
        }
    }
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err("Trophy destination is not a regular file".into());
        }
    }
    Ok(target)
}

pub(super) fn scan(app: &tauri::AppHandle, game_id: i64) -> Result<Vec<TrophyProgress>, String> {
    let conn = super::db(app)?;
    let game = super::game(&conn, game_id)?;
    let title = game
        .title_id
        .as_deref()
        .filter(|v| valid_title(v))
        .ok_or("Game has no valid title ID")?;
    let mut results = Vec::new();
    for build in super::list_builds(app.clone())? {
        results.extend(scan_build_dir(
            build.id,
            &format!("{} ({})", build.commit_sha, build.build_type),
            Path::new(&build.path),
            title,
        ));
    }
    results.sort_by(|a, b| {
        a.build_id
            .cmp(&b.build_id)
            .then(a.user_id.cmp(&b.user_id))
            .then(a.service_label.cmp(&b.service_label))
    });
    Ok(results)
}

fn scan_build_dir(
    build_id: i64,
    build_name: &str,
    build_root: &Path,
    title: &str,
) -> Vec<TrophyProgress> {
    let dir = build_root.join("_Trophies").join(title);
    let Ok(root) = fs::canonicalize(build_root) else {
        return Vec::new();
    };
    let Ok(canonical_dir) = fs::canonicalize(&dir) else {
        return Vec::new();
    };
    if !super::save_data::path_is_within(&canonical_dir, &root) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(canonical_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            let (user, service) = parse_filename(name)?;
            let (status, earned, ids, error) = match read_progress(&path) {
                Ok((ids, _)) => ("readable".to_string(), Some(ids.len()), ids, None),
                Err(e) => ("unreadable".to_string(), None, Vec::new(), Some(e)),
            };
            Some(TrophyProgress {
                id: format!("{build_id}:{user}:{service}"),
                title_id: title.to_owned(),
                build_id,
                build_name: build_name.to_owned(),
                build_path: build_root.to_string_lossy().into_owned(),
                user_id: user,
                service_label: service,
                path: path.display().to_string(),
                read_status: status,
                earned,
                earned_ids: ids,
                error,
            })
        })
        .collect()
}

fn backup_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let paths = super::state_paths(app)?;
    let conn = super::db(app)?;
    let configured: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='save_data_backup_directory'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let root = configured
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.data.join("save-backups"));
    fs::create_dir_all(root.join("trophies")).map_err(|e| e.to_string())?;
    Ok(root.join("trophies"))
}

fn copy_backup(
    app: &tauri::AppHandle,
    entry: &TrophyProgress,
    bytes: &[u8],
    label: &str,
) -> Result<String, String> {
    let root = backup_root(app)?
        .join(&entry.title_id)
        .join(entry.build_id.to_string())
        .join(format!(
            "{}-{}-{}-{}",
            entry.user_id,
            entry.service_label,
            super::now(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let hash = format!("{:x}", Sha256::digest(bytes));
    let filename = Path::new(&entry.path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid trophy filename")?;
    fs::write(root.join(filename), bytes).map_err(|e| e.to_string())?;
    let manifest = TrophyManifest {
        format_version: 1,
        title_id: entry.title_id.clone(),
        user_id: entry.user_id.clone(),
        service_label: entry.service_label.clone(),
        source_build_id: entry.build_id,
        source_build: entry.build_name.clone(),
        source_path: entry.path.clone(),
        timestamp: super::now(),
        sha256: hash,
        filename: filename.to_owned(),
    };
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let _ = label;
    Ok(root.display().to_string())
}

pub(super) fn copy_for_save_backup(
    app: &tauri::AppHandle,
    game_id: i64,
    build_id: i64,
    title_id: &str,
    save_user: Option<&str>,
    destination: &Path,
) -> Result<Vec<super::save_data::TrophyBundleFile>, String> {
    let user = save_user
        .filter(|s| s.parse::<u32>().is_ok())
        .ok_or("Combined backups require a known numeric save user ID")?;
    let entries = scan(app, game_id)?
        .into_iter()
        .filter(|e| e.build_id == build_id && e.title_id == title_id && e.user_id == user)
        .collect::<Vec<_>>();
    if entries.iter().any(|e| e.read_status != "readable") {
        return Err(
            "Combined backup cannot include an unreadable trophy file for this user".into(),
        );
    }
    if entries.is_empty() {
        return Err("No local trophy progress exists for this same build and user".into());
    }
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for entry in entries {
        let filename = Path::new(&entry.path)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid trophy filename")?;
        let (file_user, _) = parse_filename(filename).ok_or("Invalid trophy filename")?;
        if file_user != user {
            return Err("Trophy account does not match the save account".into());
        }
        let bytes = fs::read(&entry.path).map_err(|e| e.to_string())?;
        read_progress(Path::new(&entry.path))?;
        fs::write(destination.join(filename), &bytes).map_err(|e| e.to_string())?;
        result.push(super::save_data::TrophyBundleFile {
            filename: filename.to_owned(),
            checksum_sha256: format!("{:x}", Sha256::digest(&bytes)),
        });
    }
    Ok(result)
}

pub(super) fn validate_save_bundle_trophies(
    source: &Path,
    user_id: Option<&str>,
    files: &[super::save_data::TrophyBundleFile],
) -> Result<(), String> {
    if files.is_empty() {
        return Err("Combined manifest has no trophy files".into());
    }
    let user = user_id
        .filter(|s| s.parse::<u32>().is_ok())
        .ok_or("Combined trophy restore has no valid user ID")?;
    for file in files {
        let (file_user, _) = parse_filename(&file.filename)
            .ok_or("Combined manifest contains an invalid trophy filename")?;
        if file_user != user {
            return Err("Combined trophy user does not match the save user".into());
        }
        let payload = source.join(&file.filename);
        let bytes = fs::read(&payload).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(&bytes)) != file.checksum_sha256 {
            return Err("Combined trophy checksum verification failed".into());
        }
        read_progress(&payload)?;
    }
    Ok(())
}

pub(super) fn restore_save_bundle_trophies(
    app: &tauri::AppHandle,
    game_id: i64,
    build_id: i64,
    title_id: &str,
    user_id: Option<&str>,
    source: &Path,
    files: &[super::save_data::TrophyBundleFile],
) -> Result<(), String> {
    validate_save_bundle_trophies(source, user_id, files)?;
    let conn = super::db(app)?;
    let build = super::build(&conn, build_id)?;
    let root = PathBuf::from(&build.path);
    let mut staged = Vec::<(PathBuf, PathBuf, Option<PathBuf>)>::new();
    let result = (|| -> Result<(), String> {
        for file in files {
            if !valid_progress_filename(&file.filename) {
                return Err("Combined manifest contains an invalid trophy filename".into());
            }
            let payload = source.join(&file.filename);
            let bytes = fs::read(&payload).map_err(|e| e.to_string())?;
            let target = prepare_target(&root, title_id, &file.filename)?;
            let temp = target.with_extension(format!("trophy-stage-{}.tmp", super::now()));
            fs::write(&temp, &bytes).map_err(|e| e.to_string())?;
            staged.push((temp, target, None));
            let index = staged.len() - 1;
            let target = staged[index].1.clone();
            let old = if target.exists() {
                let current = scan(app, game_id)?
                    .into_iter()
                    .find(|e| e.path == target.to_string_lossy())
                    .ok_or("Existing trophy destination is not a verified file")?;
                if current.read_status != "readable" {
                    return Err(
                        "Existing trophy progress is unreadable; refusing to overwrite it".into(),
                    );
                }
                let current_bytes = fs::read(&target).map_err(|e| e.to_string())?;
                copy_backup(app, &current, &current_bytes, "pre-combined-restore")?;
                Some(target.with_extension(format!("trophy-old-{}.tmp", super::now())))
            } else {
                None
            };
            staged[index].2 = old;
        }
        for (temp, target, old) in &staged {
            if let Some(old) = old {
                fs::rename(target, old).map_err(|e| e.to_string())?;
            }
            if let Err(e) = fs::rename(temp, target) {
                if let Some(old) = old {
                    let _ = fs::rename(old, target);
                }
                return Err(format!("Combined trophy install failed: {e}"));
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        for (temp, target, old) in staged.iter().rev() {
            let installed = !temp.exists();
            let _ = fs::remove_file(temp);
            if let Some(old) = old {
                if old.exists() {
                    let _ = fs::remove_file(target);
                    let _ = fs::rename(old, target);
                } else if installed {
                    let _ = fs::remove_file(target);
                }
            } else if installed {
                let _ = fs::remove_file(target);
            }
        }
        return Err(error);
    }
    for (_, _, old) in staged {
        if let Some(old) = old {
            let _ = fs::remove_file(old);
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub(super) struct TrophyBackupRecord {
    path: String,
    title_id: String,
    user_id: String,
    service_label: String,
    source_build: String,
    source_build_id: i64,
    timestamp: i64,
    sha256: String,
    filename: String,
}

#[tauri::command]
pub(super) fn list_trophy_backups(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<Vec<TrophyBackupRecord>, String> {
    let conn = super::db(&app)?;
    let game = super::game(&conn, game_id)?;
    let title = game
        .title_id
        .as_deref()
        .filter(|s| valid_title(s))
        .ok_or("Game has no valid title ID")?;
    let root = backup_root(&app)?.join(title);
    let mut out = Vec::new();
    let Ok(build_dirs) = fs::read_dir(root) else {
        return Ok(out);
    };
    for build_dir in build_dirs.flatten() {
        let Ok(records) = fs::read_dir(build_dir.path()) else {
            continue;
        };
        for record in records.flatten() {
            let dir = record.path();
            let Ok(m) = fs::read(dir.join("manifest.json")) else {
                continue;
            };
            let Ok(m) = serde_json::from_slice::<TrophyManifest>(&m) else {
                continue;
            };
            if m.title_id != title {
                continue;
            }
            if m.filename != format!("trophies_{}_{}.json", m.user_id, m.service_label)
                || build_dir
                    .file_name()
                    .to_str()
                    .and_then(|x| x.parse::<i64>().ok())
                    != Some(m.source_build_id)
            {
                continue;
            }
            let data = dir.join(&m.filename);
            let Ok(bytes) = fs::read(&data) else { continue };
            if format!("{:x}", Sha256::digest(&bytes)) != m.sha256 || read_progress(&data).is_err()
            {
                continue;
            }
            out.push(TrophyBackupRecord {
                path: dir.display().to_string(),
                title_id: m.title_id,
                user_id: m.user_id,
                service_label: m.service_label,
                source_build: m.source_build,
                source_build_id: m.source_build_id,
                timestamp: m.timestamp,
                sha256: m.sha256,
                filename: m.filename,
            });
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
    Ok(out)
}

#[tauri::command]
pub(super) fn restore_trophy_backup(
    app: tauri::AppHandle,
    game_id: i64,
    backup_path: String,
) -> Result<String, String> {
    let _guard = ensure_idle(&app, game_id)?;
    let allowed = list_trophy_backups(app.clone(), game_id)?;
    let record = allowed
        .iter()
        .find(|r| r.path == backup_path)
        .ok_or("Trophy backup is not verified")?;
    let data = PathBuf::from(&record.path).join(&record.filename);
    let bytes = fs::read(&data).map_err(|e| e.to_string())?;
    read_progress(&data)?;
    let conn = super::db(&app)?;
    let build = super::build(&conn, record.source_build_id)?;
    let target = prepare_target(Path::new(&build.path), &record.title_id, &record.filename)?;
    if target.exists() {
        let entries = scan(&app, game_id)?;
        let existing = entries
            .into_iter()
            .find(|e| e.path == target.to_string_lossy())
            .ok_or("Existing trophy destination is not a verified progress file")?;
        if existing.read_status != "readable" {
            return Err("Existing trophy progress is unreadable; refusing to overwrite it".into());
        }
        let old = fs::read(&target).map_err(|e| e.to_string())?;
        copy_backup(&app, &existing, &old, "pre-restore")?;
    }
    let parent = target.parent().ok_or("Invalid trophy destination")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(".restore-{}.tmp", super::now()));
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    read_progress(&tmp)?;
    let old = if target.exists() {
        let backup = parent.join(format!(".restore-original-{}.tmp", super::now()));
        fs::rename(&target, &backup).map_err(|e| e.to_string())?;
        Some(backup)
    } else {
        None
    };
    if let Err(e) = fs::rename(&tmp, &target) {
        if let Some(old) = old {
            let _ = fs::rename(old, &target);
        }
        let _ = fs::remove_file(tmp);
        return Err(format!(
            "Could not install trophy backup; original file restored: {e}"
        ));
    }
    if let Some(old) = old {
        let _ = fs::remove_file(old);
    }
    Ok(target.display().to_string())
}

fn ensure_idle(
    app: &tauri::AppHandle,
    game_id: i64,
) -> Result<std::sync::MutexGuard<'static, ()>, String> {
    let guard = super::SESSION_LIFECYCLE_LOCK
        .lock()
        .map_err(|_| "Session lifecycle is unavailable")?;
    let active: bool = super::db(app)?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE game_id=? AND ended_at IS NULL)",
            [game_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if active {
        return Err("Trophy operation is unavailable during an active game session".into());
    }
    Ok(guard)
}

#[tauri::command]
pub(super) fn list_trophy_progress(
    app: tauri::AppHandle,
    game_id: i64,
) -> Result<Vec<TrophyProgress>, String> {
    scan(&app, game_id)
}

#[tauri::command]
pub(super) fn backup_trophy_progress(
    app: tauri::AppHandle,
    game_id: i64,
    path: String,
) -> Result<String, String> {
    let _guard = ensure_idle(&app, game_id)?;
    let entry = scan(&app, game_id)?
        .into_iter()
        .find(|e| e.path == path)
        .ok_or("Trophy progress path is not registered for this game")?;
    let bytes = fs::read(&entry.path).map_err(|e| e.to_string())?;
    read_progress(Path::new(&entry.path))?;
    copy_backup(&app, &entry, &bytes, "manual")
}

#[tauri::command]
pub(super) fn export_trophy_progress(
    app: tauri::AppHandle,
    game_id: i64,
    path: String,
    destination: String,
) -> Result<String, String> {
    let _guard = ensure_idle(&app, game_id)?;
    let entry = scan(&app, game_id)?
        .into_iter()
        .find(|e| e.path == path)
        .ok_or("Trophy progress path is not registered for this game")?;
    read_progress(Path::new(&entry.path))?;
    let out = PathBuf::from(&destination);
    if out.exists() {
        return Err("Export destination already exists".into());
    }
    fs::copy(&entry.path, &out).map_err(|e| e.to_string())?;
    Ok(out.display().to_string())
}

#[tauri::command]
pub(super) fn import_trophy_progress(
    app: tauri::AppHandle,
    game_id: i64,
    source: String,
    build_id: i64,
) -> Result<String, String> {
    let _guard = ensure_idle(&app, game_id)?;
    let source = fs::canonicalize(source).map_err(|e| e.to_string())?;
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid trophy filename")?;
    let _identity = parse_filename(name)
        .ok_or("Filename must preserve trophies_<USER_ID>_<SERVICE_LABEL>.json")?;
    let (ids, _) = read_progress(&source)?;
    let _ = ids;
    let conn = super::db(&app)?;
    let game = super::game(&conn, game_id)?;
    let title = game
        .title_id
        .as_deref()
        .filter(|v| valid_title(v))
        .ok_or("Game has no valid title ID")?;
    let build = super::build(&conn, build_id)?;
    let target = prepare_target(Path::new(&build.path), title, name)?;
    let registered = super::list_builds(app.clone())?
        .iter()
        .any(|b| b.id == build_id);
    if !registered {
        return Err("Target build is not registered".into());
    }
    if target.exists() {
        return Err("Target already has trophy progress for this account and service; use Restore after reviewing the source".into());
    }
    let parent = target.parent().ok_or("Invalid trophy target")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".trophy-import-{}.tmp", super::now()));
    fs::copy(source, &temp).map_err(|e| e.to_string())?;
    fs::rename(temp, &target).map_err(|e| e.to_string())?;
    Ok(target.display().to_string())
}

#[tauri::command]
pub(super) fn open_trophy_folder(
    app: tauri::AppHandle,
    game_id: i64,
    path: String,
) -> Result<(), String> {
    let entry = scan(&app, game_id)?
        .into_iter()
        .find(|e| e.path == path)
        .ok_or("Trophy progress path is not registered for this game")?;
    super::open_directory(
        Path::new(&entry.path)
            .parent()
            .ok_or("Invalid trophy folder")?,
    )
}

pub(super) fn backup_build_trophies(app: &tauri::AppHandle, build_id: i64) -> Result<(), String> {
    let conn = super::db(app)?;
    let title_ids: Vec<(i64, String)> = conn
        .prepare("SELECT id,title_id FROM games WHERE title_id IS NOT NULL")
        .map_err(|e| e.to_string())?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
    for (game_id, title) in title_ids {
        for entry in scan(app, game_id)?
            .into_iter()
            .filter(|e| e.build_id == build_id)
        {
            let bytes = fs::read(&entry.path).map_err(|e| e.to_string())?;
            read_progress(Path::new(&entry.path))?;
            copy_backup(app, &entry, &bytes, "before-build-removal")?;
        }
        let _ = title;
    }
    Ok(())
}

pub(super) fn auto_backup_after_session(app: &tauri::AppHandle, game_id: i64) {
    let enabled = super::db(app)
        .ok()
        .and_then(|c| {
            c.query_row(
                "SELECT value FROM settings WHERE key='save_data_auto_backup'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
        })
        .is_some_and(|v| v == "true");
    if !enabled {
        return;
    }
    if let Ok(_guard) = super::SESSION_LIFECYCLE_LOCK.lock() {
        if let Ok(entries) = scan(app, game_id) {
            for entry in entries.into_iter().filter(|e| e.read_status == "readable") {
                if let Ok(bytes) = fs::read(&entry.path) {
                    let _ = copy_backup(app, &entry, &bytes, "after-session");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separates_filename_user_and_service() {
        assert_eq!(
            parse_filename("trophies_1000_2.json"),
            Some(("1000".into(), "2".into()))
        );
        assert_eq!(
            parse_filename("trophies_1000_2_extra.json"),
            Some(("1000".into(), "2_extra".into()))
        );
        assert!(parse_filename("trophies_user_2.json").is_none());
    }
    #[test]
    fn parses_preserving_json_ids() {
        let d = std::env::temp_dir().join(format!("kyty-trophy-{}.json", super::super::now()));
        fs::write(
            &d,
            r#"{"unlockedTrophies":[1,"02"],"unlockedAt":{"1":123},"future":{"x":1}}"#,
        )
        .unwrap();
        let (ids, _) = read_progress(&d).unwrap();
        assert_eq!(ids, vec!["1", "02"]);
        fs::remove_file(d).unwrap();
    }
    #[test]
    fn rejects_corrupt_progress() {
        let d =
            std::env::temp_dir().join(format!("kyty-trophy-invalid-{}.json", super::super::now()));
        fs::write(&d, "{bad").unwrap();
        assert!(read_progress(&d).is_err());
        fs::remove_file(d).unwrap();
    }

    #[test]
    fn scans_real_build_layouts_without_merging_users_or_services() {
        let base = std::env::temp_dir().join(format!(
            "kyty-trophy-scan-{}-{}",
            std::process::id(),
            super::super::now()
        ));
        let b1 = base.join("build-one");
        let b2 = base.join("build-two");
        let p1 = b1.join("_Trophies/CUSA00001");
        let p2 = b2.join("_Trophies/CUSA00001");
        fs::create_dir_all(&p1).unwrap();
        fs::create_dir_all(&p2).unwrap();
        fs::write(
            p1.join("trophies_1000_2.json"),
            r#"{"unlockedTrophies":[1,2],"unlockedAt":{"1":123},"futureField":{"retained":true}}"#,
        )
        .unwrap();
        fs::write(
            p1.join("trophies_1001_2.json"),
            r#"{"unlockedTrophies":[3]}"#,
        )
        .unwrap();
        fs::write(
            p2.join("trophies_1000_3.json"),
            r#"{"unlockedTrophies":[4]}"#,
        )
        .unwrap();
        fs::write(p2.join("trophies_1000_4.json"), "{broken").unwrap();
        fs::write(
            p2.join("trophies_1000_5.json"),
            r#"{"unlockedTrophies":[]}"#,
        )
        .unwrap();
        let first = scan_build_dir(11, "build one", &b1, "CUSA00001");
        let second = scan_build_dir(12, "build two", &b2, "CUSA00001");
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 3);
        assert!(first
            .iter()
            .any(|e| e.user_id == "1000" && e.service_label == "2" && e.earned == Some(2)));
        assert!(first
            .iter()
            .any(|e| e.user_id == "1001" && e.service_label == "2" && e.earned == Some(1)));
        assert!(second
            .iter()
            .any(|e| e.service_label == "3" && e.earned == Some(1)));
        assert!(second.iter().any(|e| e.service_label == "4"
            && e.earned.is_none()
            && e.read_status == "unreadable"));
        assert!(scan_build_dir(13, "empty", &base.join("empty"), "CUSA00001").is_empty());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn combined_restore_fixture_checks_account_service_and_payload_hash() {
        let root = std::env::temp_dir().join(format!(
            "kyty-combined-trophy-{}-{}",
            std::process::id(),
            super::super::now()
        ));
        fs::create_dir_all(&root).unwrap();
        let name = "trophies_1000_4.json";
        let contents = r#"{"unlockedTrophies":[7],"unlockedAt":{"7":123},"unknown":{"keep":true}}"#;
        fs::write(root.join(name), contents).unwrap();
        let file = super::super::save_data::TrophyBundleFile {
            filename: name.into(),
            checksum_sha256: format!("{:x}", Sha256::digest(contents.as_bytes())),
        };
        assert!(
            validate_save_bundle_trophies(&root, Some("1000"), std::slice::from_ref(&file)).is_ok()
        );
        assert!(
            validate_save_bundle_trophies(&root, Some("1001"), std::slice::from_ref(&file))
                .is_err()
        );
        let invalid = super::super::save_data::TrophyBundleFile {
            filename: "trophies_1000_4.json".into(),
            checksum_sha256: "wrong".into(),
        };
        assert!(validate_save_bundle_trophies(&root, Some("1000"), &[invalid]).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
