# KytyPS5 Studio

Linux desktop manager for independent KytyPS5 source checkouts, builds, game paths, and launch diagnostics. KytyPS5 Studio does not include games, firmware, or an emulator binary.

## Current functionality

- Tauri 2 desktop shell with Home, Library, Emulators, Fork Manager, Community Builds, Build Studio, Diagnostics, and Settings.
- Home features the most recently played imported game (or an imported game when there is no play history), uses local `pic0.png` artwork when available, and launches or opens that game’s details directly.
- Renderer failures are contained by a visible recovery screen with a reload action; backend and job errors continue to surface in the app notices/logs.
- GitHub fork discovery and custom GitHub HTTPS repositories.
- Community Builds lists real pull requests for the selected tracked repository with GitHub counts, state filters, title/number/author search, label filtering, sorting, pagination, PR descriptions, commit lists, changed-file patches, checks, mergeability state, and direct GitHub links. Compatibility remains explicitly unverified until an actual build is registered.
- Optional GitHub credentials are stored through Linux Secret Service (`secret-tool`); anonymous read-only access remains available. Token values are never persisted in SQLite.
- Community PR builds pin a shared base SHA and each exact PR head SHA, support user-controlled merge ordering, create an isolated clone, perform deterministic Git merges, report conflict files and Git errors, validate submodule paths/HTTPS GitHub URLs, and use the existing CMake/Ninja/Clang build, cancellation, executable verification, build registration, and game launcher services. Before compilation Studio displays changed files (flagging build scripts and submodule configuration) and requires a separate confirmation; community build code executes and a Git clone is not a security sandbox.
- Community build history preserves the selected PR order and SHAs, configuration, build/install result, and failure details. Verified experimental builds appear in Emulators and can be activated or assigned per game through existing controls. History compares selected PR SHAs with currently loaded GitHub results and flags changed heads; rebuilding is always explicit.
- SQLite migration v9 adds community PR cache, configuration, ordered PR, and build-result records while retaining existing repository, build, game, profile, and session IDs.
- Recursive Git clone with submodules into the XDG data directory.
- Checked-out source inspection before the KytyPS5 CMake/launcher adapter enables compilation; unmatched repositories remain unverified and disabled.
- Versioned CMake/Ninja/Clang build and install with command and required Qt pkg-config preflights, streamed logs, process-tree cancellation, `--help`/`--game` executable validation before registration or activation, and active-build rollback.
- Build Studio shows the active stage and elapsed time from actual job events; it does not invent percentage progress.
- Build Studio exposes clean versus incremental builds, build type, and bounded parallel jobs; the verified Linux adapter uses Clang/Clang++ and reports that requirement explicitly.
- Emulator comparison records install size and CLI-verification status for new builds; existing registrations retain an explicit unmeasured/unverified state until rechecked. The build list shows the latest recorded process outcome per build without implying gameplay success. SQLite migration v7 adds these fields while retaining existing build records.
- On startup, registered builds with an unchecked CLI status are revalidated asynchronously, active first; missing and non-executable paths are surfaced immediately from filesystem checks without delaying the initial UI render.
- KDE/Linux folder actions open imported game locations, emulator installations, and cloned repositories through `xdg-open`, using canonical registered paths and argument-safe process launching.
- Diagnostics can export managed emulator session logs through the native save dialog; exports are path-checked, size-limited, and written atomically.
- Startup and configurable periodic update checks compare each tracked branch with the newest registered build. Settings can notify only or opt in to automatic Release fetch/builds; automatic builds wait for Studio-tracked emulator sessions to finish, retain prior installations, verify the emulator CLI, skip an already verified commit, and leave activation manual. Build Studio also provides the same update operation on demand.
- Game path registration by file picker or native file drop, plus directory scanning without copying game data; scans detect game folders, signature-checked ELF files, and KytyPS5 ZArchives; local PS5 `sce_sys/param.json` title, title ID, version, `icon0.png` icons, and `pic0.png` backgrounds; a virtualized library grid with lazy-loaded, in-memory-cached local icons; `--game` launch against a verified build; per-game resolution, present mode, performance overlay, fullscreen, controller color/volume/vibration, and keyboard-player options discovered from the selected executable's real `--help` output; session logs, exit codes, and playtime. Artwork reads are constrained to the imported game directory and size-limited. The Library exposes observed path/process/title-ID evidence while explicitly leaving gameplay and playability unverified. Unsupported controls are disabled and stale unsupported values can be cleared. Versioned database migrations preserve existing records and profile values; migration v6 adds controller options with nullable defaults.
- The last UI page, selected game/repository, and library search are restored from local webview storage on startup; invalid saved pages fall back to Home and missing game/repository selections fall back to available data.
- Host diagnostics for CPU model, available/total RAM, GPU, Linux input-controller names, build tools, and XDG data paths; live emulator process CPU, resident RAM, runtime, PID, and status from Linux `/proc`.
- On-demand graphics diagnostics report Vulkan loader/device/driver details from `vulkaninfo`, connected DRM displays and modes, and loaded kernel GPU drivers. The Vulkan probe runs only when Diagnostics is opened and has a six-second process-group timeout.
- In-Studio gamepad navigation through the browser Gamepad API: D-pad/stick moves focus, A activates focused buttons, and shoulder buttons change pages. This is UI navigation only and does not imply KytyPS5 game-input compatibility.
- SQLite persistence under `$XDG_DATA_HOME/kytyps5-studio` (defaults to `~/.local/share/kytyps5-studio`).

## Build and run on Arch / CachyOS

Install the development prerequisites using your package manager. The upstream emulator currently needs `git cmake ninja clang lld glslang python qt6-base vulkan-headers pkgconf` plus SDL3 backend development libraries for X11, Wayland, audio, and udev. Studio itself needs `webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg xdg-utils libnotify libsecret` for Tauri, desktop folder actions, and KDE notifications. Confirm package names against your distribution before installing. Studio never runs a privileged package installation. Desktop notifications are best-effort and use the host's `notify-send`; in-app notices remain available if the desktop service is unavailable. On NVIDIA Wayland sessions, Studio disables WebKit's DMA-BUF renderer when the user has not set `WEBKIT_DISABLE_DMABUF_RENDERER`; this avoids a GBM buffer failure observed on the target host.

```sh
npm ci
npm run tauri dev
```

Build a native application binary:

```sh
npm run tauri build -- --no-bundle
```

The binary is at `src-tauri/target/release/kytyps5-studio`. Use `npm run tauri build -- --bundles appimage` for an AppImage. Run `./packaging/arch/build-package.sh` from the checkout to make a clean, checksummed source archive and package with `makepkg`; install the result with `sudo pacman -U packaging/arch/kytyps5-studio-0.1.0-1-x86_64.pkg.tar.zst`. The PKGBUILD disables LTO to avoid bundled ring/SQLite linker conflicts and normalizes native-only CachyOS compiler flags to baseline x86-64 for this package, without changing host configuration.

## Workflow

1. Add the official repository under Fork Manager and clone it.
2. Inspect the source adapter (the official layout is detected automatically after clone).
3. Build in Build Studio. The launcher target includes `kyty_emulator`; Studio checks `--help` and the advertised `--game` option before recording a build. Use **Fetch & build latest** to build the tracked branch update without replacing the active install.
4. Activate a verified build under Emulators when ready.
5. Scan a game folder or add an ELF/ZArchive in Library, then launch. The Studio verifies the executable format where applicable; the emulator still determines whether a particular game can initialize.

Each checkout lives in `sources/<repository id>`, reusable compiler output in the XDG cache, and each verified installation in a distinct `builds/<repository id>/<commit>-<configuration>-<timestamp>` directory. Updating source or rebuilding keeps installed builds available. Game paths always point to the originals. Build verification runs the installed executable help command and checks for its game-launch option; it does not prove game compatibility.

## Upstream research

The [current KytyPS5 Linux instructions](https://github.com/KytyPS5/KytyPS5#building-on-linux) require Clang, Ninja, CMake, Git submodules, Qt 6 for `launcher`, glslang, Python, and SDL3 system backends. The current [`src/main.cpp`](https://github.com/KytyPS5/KytyPS5/blob/main/src/main.cpp) accepts `kyty_emulator --game <dir|elf|zar>`. The official repository is GPL-2.0. [AnyPS5 Studio](https://github.com/Ramzyayman/AnyPS5-Studio) is a separate static relinker, and no relinker code is included here. [SlunkySloth/kytyps5](https://github.com/SlunkySloth/kytyps5) is a fork; it is only enabled for building if inspection finds the supported layout.

## Limits

The current release is a Linux foundation. Additional fork adapters, signed Studio updates, external metadata providers, and Windows support are not implemented. Controller detection reports Linux input devices only and does not imply game compatibility. Standard Gamepad API controllers can navigate Studio with the D-pad or left stick, A activates the focused control, and B sends Escape; the browser/WebKit must expose the controller to the app. Community PR builds currently support repositories that pass the existing KytyPS5 CMake adapter check; an adapter check is not a compatibility claim. GitHub unauthenticated rate limits apply to discovery. PR head-change notices compare loaded GitHub results and do not trigger silent rebuilds. A conflict-free merge, successful compile, and CLI validation still do not establish that a game works. No emulator or game compatibility has been certified by this project. Profile capabilities are re-read from the selected emulator executable at runtime; this verifies advertised CLI flags, not their behavior inside every game.

## Validation on CachyOS (2026-10-18)

At upstream commit `099de8dea62cbf17c8e06a6c24e72b38475d1566`, a fresh recursive checkout configured with Clang and Ninja, built the `launcher` target, and installed both `kyty_launcher` and `kyty_emulator`. The installed emulator executed `--help` successfully, and `ldd` reported no missing shared libraries. The verified installation was copied to Studio's XDG data directory and registered as the active build. `npm run build`, Rust tests, the Tauri native release build, and AppImage bundling succeeded. No game files were supplied, so actual game launch and compatibility remain unverified.
