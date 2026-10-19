# KytyPS5 Studio

KytyPS5 Studio is a Linux desktop app for managing KytyPS5 source checkouts, emulator builds, game folders, and launch logs in one place.

> **This project is not affiliated with or endorsed by the KytyPS5 developers.** The emulator source is maintained in the [official KytyPS5 repository](https://github.com/KytyPS5/KytyPS5). Studio can clone and build that source, as well as compatible forks. It does not include an emulator binary, games, or firmware.

## Screenshots

| Games | Emulator builds | Build Studio |
| --- | --- | --- |
| <img src="docs/screenshots/games-home.png" alt="KytyPS5 Studio games home page" width="320"> | <img src="docs/screenshots/emulator-builds.png" alt="Installed KytyPS5 emulator builds" width="320"> | <img src="docs/screenshots/build-studio.png" alt="KytyPS5 Studio build page" width="320"> |

The screenshots show the games home page, installed emulator builds, and the source build interface.

## What it does

- Organizes imported games without moving or copying the original files.
- Clones and manages the official KytyPS5 repository and compatible forks.
- Builds and keeps separate emulator versions, with controls to activate, compare, update, and remove them.
- Finds Community Pull Requests, reviews their changes, and can build compatible PR combinations while recording their exact commits.
- Stores per-game launch options, session history, playtime, and emulator logs.
- Shows host and Vulkan diagnostics to help troubleshoot build and launch problems.

See the [0.2.0 release notes](CHANGELOG.md) for recent changes.

Successful compilation or CLI verification does **not** guarantee that a game will run. Compatibility depends on the emulator, game, drivers, and system configuration.

## Run the AppImage

Download the Linux AppImage provided with the release, make it executable, and run it:

```sh
chmod +x "KytyPS5 Studio_0.2.0_amd64.AppImage"
./"KytyPS5 Studio_0.2.0_amd64.AppImage"
```

The AppImage filename may change between releases. KytyPS5 emulator builds require a working Vulkan driver. Building the emulator from Studio also requires the development tools and libraries listed in the Build Studio dependency check.

## Build from source

On Linux, install Node.js/npm, stable Rust, and the Tauri system dependencies for your distribution. Then run:

```sh
npm ci
npm run tauri dev
```

To create a native AppImage:

```sh
npm run tauri build -- --bundles appimage
```

The AppImage is written to:

```text
src-tauri/target/release/bundle/appimage/
```

For Arch Linux and CachyOS, the repository also includes a package recipe at [`packaging/arch/PKGBUILD`](packaging/arch/PKGBUILD).

## Game files and privacy

Game files stay at their existing paths; Studio keeps their paths and metadata in its local library. Application data is stored in the user’s XDG data directory, normally `~/.local/share/kytyps5-studio`. GitHub access works anonymously within GitHub’s rate limits; an optional token can be stored through Linux Secret Service.

Only add games and other content that you are authorized to use. This project does not provide games, firmware, decryption keys, or copyrighted content.

## AI assistance

AI tools were used to help develop and document this project. The project owner directs the work and is responsible for reviewing changes and releases. This statement does not imply that the KytyPS5 emulator itself was made with AI.

## License

KytyPS5 Studio is released under the [MIT License](LICENSE). The KytyPS5 emulator is a separate project with its own license and terms; see its [official repository](https://github.com/KytyPS5/KytyPS5).
