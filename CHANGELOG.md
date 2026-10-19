# Changelog

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
