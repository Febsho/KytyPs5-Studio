# Changelog

## 1.1.0

### Saves and patches

- Automatically carry the save from the most recently played emulator build into the build selected for the next launch, preserving a verified backup before replacing different save data.
- Keep imported and edited game patches in Studio data and restore them into the selected build after app restarts or build changes.
- Fix the automatic save-backup control layout.

### Build-specific settings

- Show launch settings advertised by each installed executable, including older builds, instead of limiting settings to the newest upstream revision.
- Refresh the selected build's capabilities when opening its settings and filter options that build marks unavailable or unsupported.

### Library and sessions

- Enlarge game logos and move version, storage, compatibility, and trophy details below the logo; remove the redundant imported-game status line.
- Refresh trophy progress cards with clearer, modern grade styling.
- Prevent a previous crashed or closed game's session summary from reopening when a later launch attempt fails.

## 1.0.0

### Build-specific emulator settings

- Verify configurable CLI options against the exact clean source revision for each installed binary; show advertised but unverified flags separately and keep them out of launch arguments.
- Key capability results to the executable SHA-256, source revision, and host OS, and display those identifiers with the selected build.
- Keep defaults per emulator build, expose unavailable settings with explanations, and validate option values in both the editor and Rust command path.
- Filter stale inherited settings before launch, Steam shortcut generation, and A/B comparison; preserve each game's saved options for compatible builds.
- Verify background shader precompile support from source usage instead of guessing from a repository name, and identify full offline precompile only when its required assets are present.
- Use the upstream `--no-trophy-notifications` switch and enforce its duration dependency.
- Accept safe nonnumeric trophy service labels while keeping the account ID numeric.

### Release

- Mark the first stable Studio release as 1.0.0. This release includes the Save Data Manager, local trophy preservation, compatibility details, and game patch management added in 0.10.0.

## 0.10.0

### Trophy preservation

- Read local earned-state JSON separately by installed build, user, and service label; malformed or missing progress remains explicitly unavailable.
- Add verified local trophy backups, restore, raw JSON import/export, and folder access; back up trophy files before build removal and after opted-in session backups.
- Add optional combined Save + Trophies backups with per-file hashes and explicit manifest inclusions; restore is restricted to the same title, build, and numeric user and preserves service labels separately.
- Document that trophy package definitions are distinct from local progress and that no PlayStation Network sync is provided.

### Save Data Manager

- Detect verified standard, guest-user, and memory save slots across installed builds.
- Add safe folder and ZIP import, ZIP export, checksummed backups, restore history, and confirmed deletion.
- Add session lockout, optional automatic session backups, configurable backup storage, and explicit retention cleanup that protects the latest verified restore point per slot.
- Document supported layouts and physical console save limitations.

## 0.9.3

### Game patches

- Fix patch option rows so checkboxes stay compact and patch names remain readable across the available panel width.

## 0.9.2

### Game library and trophies

- Show official compatibility status consistently across game cards, the library, and the game overview, and simplify the selected-game sidebar.
- Organize trophy progress into Bronze, Silver, Gold, and Platinum counts on the Home screen and in the library.
- Improve game overview details and emulator build naming.

### Emulator builds and shaders

- Make emulator update checks actionable and keep fork updates under the user's control.
- Show shader precompile progress and support safely stopping and resuming work.
- Improve storage cleanup and app update/restart handling.

### Game patches

- Import and manage game patch files, including enabling or disabling patch options for supported games.

## 0.9.1

### Build reliability

- Fix Linux x86_64 emulator source builds with upstream's Microsoft-specific `intrin.h` include.

## 0.9.0

### Emulator settings

- Read CLI option types, choices, defaults, ranges, and descriptions from each installed build so reported settings use usable selectors instead of plain text fields.
- Improve path selection and validation for settings dialogs, and prevent form buttons from accidentally submitting the settings form.
- Keep common game-profile settings and build defaults in sync with the same per-build option metadata.

### Shader precompilation

- Stage a fork's bundled shader precompile assets when installing or building it, and detect supported precompile tools per build and platform.
- Add a per-game control to enable background shader precompilation on supported forks; keep the full manual precompile action available when the selected build provides it.
- Report unsupported precompile modes accurately instead of treating background shader recording as full offline precompilation.

### Game details

- Improve settings and compatibility page layout and expose the current emulator build's real game-profile options in game details.

## 0.8.0

### Shader precompilation

- Added a per-build background shader-precompile setting for the Jetsku KytyPS5 fork. Supported builds launch with shader precompile and fast-first pipeline preparation enabled; Windows defaults on, while Linux remains opt-in because it has not been verified here.
- Added a full precompile action in game details when the selected build includes its platform-specific script and shader precompile tool, with live output and build-local cache information.
- Prevented launching or precompiling concurrently, and kept precompile caches within the selected emulator build so the fork can key them by game and graphics driver.
- Clarified that shader optimization and shader precompilation are separate settings.

## 0.7.0

### Build comparison and sessions

- Added a per-game build comparison based on recorded emulator sessions, including launch outcome, exit code, duration, and log access. FPS, frame times, and game phase are shown only when actually recorded.
- Added an A/B launch flow that runs the same game with two installed builds and shared settings, and stops when the builds cannot accept the same options.
- Improved session history and made it easier to inspect the logs associated with a build run.

### Crash diagnostics

- Added Smart Crash Analyzer log grouping for common import, Vulkan, shader, memory, and configuration errors.
- Added actionable diagnostic hints and export of a Markdown report for GitHub issues.

### Emulator settings

- Settings now target a selected installed emulator build and show only options reported by that build.
- Put common graphics options first and collapsed the remaining reported options by default.
- Removed Studio-created resolution presets and fixed option selectors so they do not offer values the emulator has not reported.

## 0.6.0

### Games and Steam

- Show the PS5 `pic2` game logo over the game artwork when available, with cached DDS conversion and fallbacks for unsupported or missing logos.
- Update an existing Steam shortcut when adding a game again with a different emulator build, instead of leaving a duplicate shortcut behind.
- Use the Studio icon as Steam artwork when a game does not include a cover image.
- Improve keyboard focus and Escape handling in the Steam build selection dialog.

### Updates and build reliability

- Improve Linux AppImage update handling.
- Pin the KytyPS5 revision used by regression builds and add a separate scheduled workflow to check the latest upstream build and CLI.
- Move Steam shortcut creation and VDF handling into dedicated Rust modules.

### Dialogs

- Replace browser confirmation prompts with native Tauri confirmation dialogs and add a Studio-styled dialog for renaming community build configurations.

## 0.5.0

### Build Studio and forks

- Clone and restore workflows now initialize Git submodules automatically, and Build Studio handles submodules as part of each build.
- Added clearer update status and a direct action to fetch the latest fork source and build it.
- Improved build performance by reusing an already verified build for the same fork commit and using Git's local object hardlinks for Community PR builds.
- Reduced default Ninja parallelism on memory-limited systems to avoid excessive resource use during Windows and Linux builds.
- Added clearer build dependency checks and in-app installation support for missing Linux and Windows build tools.

### Games and Steam

- Improved game-library scanning and loading performance, including more reliable game detection on Windows.
- Added controller navigation, scrolling, and game launching from the Home game row.
- Added **Open game folder** to the game right-click menu.
- Reworked **Add to Steam** to select an installed fork/build and preview the game artwork and launch details. Steam shortcuts now use the selected emulator build and saved game options.

### Game cheats

- Improved online cheat lookup by normalizing title IDs and allowing valid results when individual repository files are unavailable.
- JSON imports now validate and select the patch for the game and load the imported JSON into the editor.
- Downloaded cheat files can be selected for the game with **Download & use**.

### Repository management

- Improved update-check feedback and display of installed and available commits, with a direct build action when an update is available.
- Clarified that Build Studio manages submodules during builds.

## 0.4.2

### Windows and Linux build tools

- Added in-app installation for missing emulator build tools on Windows and Linux, with progress and command output shown inside Studio.
- Prevented installer, Git, and log actions from opening separate command or PowerShell windows on Windows.
- Fixed handling of already-installed LLVM packages and improved dependency checks before builds start.

### Repository and community builds

- Improved clone and restore safety, including protection against repeated actions and incomplete checkouts.
- Fixed submodule initialization and validation before building repository forks.
- Fixed Windows paths used for isolated Community PR build clones.

### Games, Steam, and responsiveness

- Improved Windows game library loading and navigation responsiveness, including artwork and metadata handling.
- Added library artwork and corrected launch details when adding games to Steam.
- Fixed Windows storage reports to account for downloaded files.

### Controller input

- Prevented controller input from navigating Studio while a game is running.
- Temporarily hid the controller remapping menu while Windows controller detection is being addressed; the implementation remains in the app.

## 0.4.1

### Bug fixes

- Require absolute storage locations from XDG and Windows AppData environment variables, falling back to the standard user directories when a value is invalid.
- Remove compatibility labels from game covers while keeping compatibility details available elsewhere in the app.

## 0.4.0

### Windows support

- Added a native Windows 10/11 x64 build and NSIS installer, including first-run storage under LocalAppData and Roaming AppData.
- Added Windows emulator discovery and native source builds, with actionable checks for the Visual Studio, LLVM, CMake, Ninja, Git, Python, and Vulkan SDK requirements.
- Added Windows process-tree ownership, safe recovery and termination of emulator sessions, Windows Credential Manager storage, host diagnostics, Discord IPC, Steam shortcuts, notifications, and directory opening.
- Preserved Linux XDG paths, existing SQLite data, Linux process behavior, and case-sensitive filesystem matching.

### Release and verification

- Added Linux and Windows CI, native emulator build checks, Windows install/update smoke checks, and release artifacts for AppImage, Arch Linux, and Windows x64.
- Added cross-platform path and process handling, including Unicode, spaces, drive-letter, and UNC path coverage.

## 0.3.0

### Compatibility and recommendations

- Integrated the official KytyPS5 compatibility database with PPSA matching, Linux-first results, report details, and offline caching.
- Added the Smart Fork & Build Advisor. Recommendations require an exact Linux test report and a matching verified local build or checkout; unsupported guesses are shown as “Not enough data.”
- Added report, tested-commit, and included community PR links, with explicit controls to use or activate a verified build.

### Games and integrations

- Added game-folder scanning and single-game import choices, plus whole-folder install-size calculation.
- Added Steam shortcut integration and Discord Rich Presence with game artwork where metadata provides it.
- Refined game library cards, compatibility details, and application focus styling.

### Community builds

- Improved Community PR build handling for selected changes based on different commits on the same target branch; selected PR order is retained and conflicts stop the build.

### Notes

- Compatibility recommendations reflect published reports and exact source commits. They do not promise gameplay, and the Studio does not infer performance or stability from stars, commit activity, or PR inclusion.
- KytyPS5 Studio manages emulator sources and builds; it does not include the emulator source, games, or firmware.

## 0.2.0

### Games and library

- Refined the Games home page and library layout, including portrait covers, cleaner selection states, and horizontal scrolling.
- Improved recognized-game scanning and title metadata, with clearer trophy progress and trophy details.
- Added game context actions and custom GoldHEN-style patch import and editing.
- Removing a game entry now leaves the original game files and session history intact.

### Emulator and repository management

- Improved official-source and fork identification, clone retries, repository removal, and cleanup of builds whose files were removed outside Studio.
- Rebuilding a source updates its existing emulator build entry instead of adding a duplicate.
- Added clearer build stages, elapsed time, and copyable build and session logs.
- Improved recent-session diagnostics and crash context.

### Settings and integrations

- Added Discord Rich Presence controls and AppImage update checks.
- Added storage cleanup actions and improved global settings spacing and controls.
- Improved Linux folder selection and desktop integration.

### Notes

- KytyPS5 Studio manages the emulator source and builds; it does not include the emulator source, games, or firmware.
- Successful build or CLI verification does not guarantee game compatibility.
