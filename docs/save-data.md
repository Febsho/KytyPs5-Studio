# Save Data Manager

## Local trophy progress

Game-provided definitions are parsed from `sce_sys/trophy2` or `sce_sys/trophy` in the game package. Earned progress is a separate, local emulator file in each installed build at `_Trophies/<TITLE_ID>/trophies_<USER_ID>_<SERVICE_LABEL>.json` (the `_Trophies` path matches KytyPS5 `Common::Trophies::UnlocksPath`). Studio lists each build, user and service independently. A missing file is not treated as zero earned; malformed JSON is shown as unreadable.

The Trophies tab can back up a readable file, restore a Studio-verified backup, export the original JSON, import a validated JSON file that retains its `trophies_<USER_ID>_<SERVICE_LABEL>.json` name, and open its containing folder. Restore and import are blocked during active sessions. Import refuses to overwrite an existing user/service destination. The progress JSON is copied byte-for-byte, retaining timestamps, unknown fields and other original properties. Its contents do not embed a game title or source build, so an external raw JSON import can validate structure and the user/service filename but cannot prove where that file came from; confirm the target game before importing.

Trophy backups use the configured backup location under `trophies/<TITLE_ID>/...`, with a checksum manifest. Session auto-backup includes readable trophy progress when save auto-backup is enabled. Build removal creates trophy backups before removal and aborts if any source file cannot be verified or copied. In Save Data, “Save + Trophies” creates one backup directory with the existing save payload and the matching numeric user's trophy JSON files under `trophies/`. The existing manifest records `includes_trophies` and each included filename and SHA-256. Combined restore verifies all payloads before changing data, restores only to the recorded build and same title/user, keeps service-label files distinct, and backs up existing trophy files before replacement. Trophy definitions are not included in earned-state backups, and Studio does not sync with PlayStation Network or create unlocks.

## Verified KytyPS5 layouts

The upstream `src/libs/libSaveData.cpp` currently uses the relative root
`_SaveData`. Studio launches each emulator with `current_dir(build.path)`, so
the layouts scanned by Studio are:

- `_SaveData/<TITLE_ID>/<SAVE_SLOT>` for standard saves.
- `_SaveData/[users]/<USER_ID>/<TITLE_ID>/<SAVE_SLOT>` for guest-user saves.
- `_SaveData/<TITLE_ID>/sce_sdmemory[1..3]/<USER_ID>` for memory saves
  (the first memory slot is named `sce_sdmemory`).

Studio scans these paths for each installed build and labels each result with
the registered build, commit, title ID, layout, user when present, size and
directory modification time. It does not infer saves from game installation
files. Fork-specific alternate roots are shown only after they are added as a
verified layout; this version does not assume their existence.

## Backup format and safety

Backups default to Studio's platform application data directory in
`save-backups/<TITLE_ID>/`; users can select another existing folder outside
all registered build directories. Each backup has a
`data/` tree and a `manifest.json` with title/user, source fork/build/commit,
game version when known, source layout, timestamp, SHA-256 tree checksum,
format version, label, and `includes_trophies: false`. Unchanged trees with the
same label are deduplicated. Auto-backup after completed sessions is optional.
Build removal makes a backup of detected saves first. Cleanup is explicit and
uses the configured day threshold; it keeps the newest checksum-verified backup
for each build, user and slot.

ZIP exports contain a top-level folder named for the title ID, a
`.kytyps5-save.json` layout/checksum manifest, and the save subtree under
`payload/`. ZIP imports verify the manifest and payload checksum and require a
registered source fork and commit that match the destination build.
Directory imports require the selected directory itself to be named for the
title ID; when it comes from a registered build, fork and commit must match.
Absolute/traversing names, duplicate paths, symlinks, excessive entry counts,
large expansions and extreme compression ratios are rejected. Directory imports
require the selected directory itself to be named for the title ID. Existing
data is backed up before replacement, and replacement uses a staged directory
with rollback on install errors. Game sessions lock save mutations.

Exports contain the complete selected save subtree under a title-ID folder.
Backups and exports do not include trophies. Physical PS5 saves are not
importable unless a real decryption/conversion method is supplied. Cross-build
migration is blocked unless the registered fork and commit match. Save
directories or roots outside the layouts above are reported as undetected;
unknown fork layouts and alternate roots remain unverified.
