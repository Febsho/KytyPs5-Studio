# Changelog

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
