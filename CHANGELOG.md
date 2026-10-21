# Changelog

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
