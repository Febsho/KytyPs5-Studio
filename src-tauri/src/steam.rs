use super::*;
use super::steam_shortcuts::*;

pub(super) fn add_game_to_steam(
    app: tauri::AppHandle,
    game_id: i64,
    build_id: i64,
) -> Result<String, String> {
    #[cfg(unix)]
    let steam_running = fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            fs::read_to_string(entry.path().join("comm"))
                .ok()
                .is_some_and(|name| matches!(name.trim(), "steam" | "steam-runtime"))
        });
    #[cfg(windows)]
    let steam_running = windows_process::process_name_exists("steam.exe")?;
    if steam_running {
        return Err("Close Steam completely, then add the game again so Steam cannot overwrite its shortcut list.".into());
    }
    let c = db(&app)?;
    let g = game(&c, game_id)?;
    // Steam shortcuts should remain usable for homebrew and dumps without
    // sce_sys/icon0.png. Use the app icon as a stable placeholder in that case.
    let image = game_artwork_root(Path::new(&g.path))
        .and_then(|root| fs::canonicalize(root).ok())
        .and_then(|root| {
            let candidate = g
                .icon_path
                .as_deref()
                .map(Path::new)
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.join("sce_sys/icon0.png"));
            let image = fs::canonicalize(candidate).ok()?;
            (path_starts_with(&image, &root) && image.is_file()).then_some(image)
        });
    let game_profile = profile(&c, game_id)?;
    let b = build(&c, build_id)?;
    let capabilities = capabilities_for_build(&b)?;
    let mut options = effective_profile_options(
        &global_profile_options(&c)?,
        &game_profile.options,
        &capabilities,
    )?;
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
        return Err("The selected emulator build no longer supports this game's saved profile. Clear unsupported profile options before adding it to Steam.".into());
    }
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
    let exe = emulator_executable(Path::new(&b.path));
    if !is_emulator_executable(&exe) {
        return Err("The selected emulator build is missing or not executable".into());
    }
    let game_path = fs::canonicalize(&g.path)
        .map_err(|_| "The registered game path is missing or inaccessible".to_string())?;
    if !supported_game_path(&game_path) {
        return Err(
            "The registered game path no longer contains a supported eboot.bin, ELF, or ZArchive"
                .into(),
        );
    }
    let args = extra_profile_arguments(&options, &capabilities.switch_options);
    let config = steam_config_dir()?;
    #[cfg(unix)]
    let (exe_field, start_dir, launch_options) = {
        let launcher_dir = state_paths(&app)?.data.join("steam-launchers");
        fs::create_dir_all(&launcher_dir).map_err(|e| e.to_string())?;
        let launcher = launcher_dir.join(format!("game-{game_id}.sh"));
        let mut script = format!(
            "#!/bin/sh\nexec {} --game {}",
            steam_shell_quote(&exe.display().to_string()),
            steam_shell_quote(&game_path.display().to_string())
        );
        for arg in args {
            script.push(' ');
            script.push_str(&steam_shell_quote(&arg.to_string_lossy()));
        }
        script.push('\n');
        fs::write(&launcher, script)
            .map_err(|e| format!("Could not create Steam game launcher: {e}"))?;
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let exe_field = format!("\"{}\"", launcher.display());
        (
            exe_field,
            format!("\"{}\"", launcher_dir.display()),
            String::new(),
        )
    };
    #[cfg(windows)]
    let (exe_field, start_dir, launch_options) = {
        let exe_field = format!("\"{}\"", steam_windows_path(&exe));
        let game_path = steam_windows_path(&game_path);
        let mut arguments = vec![OsString::from("--game"), OsString::from(&game_path)];
        arguments.extend(args);
        let launch_options = arguments
            .iter()
            .map(|argument| windows_command_line_quote(&argument.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");
        let start_dir = format!(
            "\"{}\"",
            steam_windows_path(exe.parent().unwrap_or(Path::new(&b.path)))
        );
        (exe_field, start_dir, launch_options)
    };

    let app_name = format!("{} · KytyPS5 Studio", g.title);
    let app_id = steam_crc32(format!("{exe_field}{app_name}").as_bytes()) | 0x8000_0000;
    let grid = config.join("grid");
    fs::create_dir_all(&grid).map_err(|e| format!("Could not create Steam artwork folder: {e}"))?;
    let artwork = grid.join(format!("{app_id}p.png"));
    if let Some(image) = image {
        fs::copy(&image, &artwork)
            .map_err(|e| format!("Could not copy cover image to Steam: {e}"))?;
    } else {
        fs::write(&artwork, include_bytes!("../icons/icon.png"))
            .map_err(|e| format!("Could not write placeholder artwork to Steam: {e}"))?;
    }
    let shortcuts_path = config.join("shortcuts.vdf");
    let mut shortcuts = if shortcuts_path.exists() {
        fs::read(&shortcuts_path).map_err(|e| e.to_string())?
    } else {
        b"\0shortcuts\0\0\x08\x08".to_vec()
    };
    {
        let mut entry = vec![0];
        entry.extend_from_slice(b"0\0");
        steam_vdf_int(&mut entry, "appid", app_id);
        steam_vdf_string(&mut entry, "AppName", &app_name)?;
        steam_vdf_string(&mut entry, "Exe", &exe_field)?;
        steam_vdf_string(&mut entry, "StartDir", &start_dir)?;
        steam_vdf_string(&mut entry, "icon", &artwork.display().to_string())?;
        steam_vdf_string(&mut entry, "ShortcutPath", "")?;
        steam_vdf_string(&mut entry, "LaunchOptions", &launch_options)?;
        steam_vdf_int(&mut entry, "IsHidden", 0);
        steam_vdf_int(&mut entry, "AllowDesktopConfig", 1);
        steam_vdf_int(&mut entry, "AllowOverlay", 1);
        steam_vdf_int(&mut entry, "OpenVR", 0);
        steam_vdf_int(&mut entry, "Devkit", 0);
        steam_vdf_string(&mut entry, "DevkitGameID", "")?;
        steam_vdf_int(&mut entry, "DevkitOverrideAppID", 0);
        steam_vdf_int(&mut entry, "LastPlayTime", 0);
        steam_vdf_string(&mut entry, "FlatpakAppID", "")?;
        steam_vdf_string(&mut entry, "sortas", &app_name)?;
        entry.push(0);
        entry.extend_from_slice(b"tags\0");
        entry.push(8);
        entry.push(8);
        steam_upsert_shortcut(&mut shortcuts, &app_name, &entry)?;
        let temporary =
            shortcuts_path.with_file_name(format!("shortcuts.vdf.tmp-{}", std::process::id()));
        let write_result = (|| {
            use std::io::Write as _;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&shortcuts)?;
            file.sync_all()?;
            drop(file);
            replace_file(&temporary, &shortcuts_path)
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary);
            return Err(format!("Could not update Steam shortcuts: {error}"));
        }
    }
    Ok(format!("Added {} to Steam with its included cover and selected emulator build. Restart Steam to see it in your library.", g.title))
}

