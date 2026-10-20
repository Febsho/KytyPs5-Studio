# Cross-platform development rules

KytyPS5 Studio is one shared application with feature parity on Linux and
Windows 10/11 x64. Preserve Linux behavior and data while implementing Windows
support; do not add a separate Windows application or silently disable a
feature on either platform.

## Platform support

- Keep platform-specific code behind `cfg` boundaries and expose shared
  interfaces for paths, process management, notifications, directory opening,
  credentials, Discord IPC, Steam integration, and host diagnostics.
- Linux must retain XDG paths and existing SQLite data. Windows must use the
  appropriate LocalAppData/Roaming AppData locations and preserve paths that
  include drive letters, UNC roots, Unicode, and spaces. Respect the host
  filesystem's case rules: Windows filename matching should be
  case-insensitive where appropriate, while Linux behavior remains
  case-sensitive.
- Windows application-owned children must start suspended, join a kill-on-close
  Job Object, and only then resume. Validate process identity before stopping
  recovered sessions. Preserve cancellation, timeout, cleanup, resource
  tracking, and session persistence semantics on both platforms.
- Report missing host tools and unsupported host capabilities with actionable
  errors. Never use placeholder implementations or fabricated compatibility,
  build, or performance data.
- Keep emulator discovery, validation, installation, source builds, GitHub
  integrations, compatibility recommendations, Discord presence, Steam
  shortcuts, notifications, diagnostics, credentials, and updates functional
  on both platforms.

## Changes and verification

- Every feature, bug fix, and refactor must be reviewed for Linux and Windows
  behavior before merging.
- Run frontend tests, TypeScript checks, Rust tests, formatting, and Clippy
  where applicable. Verify production packaging on Linux and Windows when
  packaging or release behavior changes.
- GitHub Actions must run relevant automated checks on both Linux and Windows
  for every pull request and build native release artifacts for both platforms
  on version tags.
- Add platform-specific regression tests for platform-specific behavior; keep
  shared behavior covered on both platforms.
- Do not claim a platform is verified unless its CI or native environment has
  actually completed the relevant checks successfully.
