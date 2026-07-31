# 0003: Publish tagged macOS releases with a GitHub-backed updater

- Status: Accepted
- Date: 2026-07-30

## Context

Void needs an installable first release and an in-application update flow.
Releases must be created only by explicitly pushing a release tag. The first
supported distribution target is Apple-silicon macOS.

Sunware commit `ae562ceff699152c15558ee062f293acacd2de35` uses a
`v*.*.*` GitHub Actions trigger, signed and notarized macOS DMGs, and GitHub
Releases as its stable update source. Zed commit
`5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f` uses channel-specific application
identifiers, an hourly background updater, platform-specific release assets,
DMG mounting, `rsync` replacement of the running app, explicit unmounting, and
GPUI restart. Zed's feed itself is served by Zed infrastructure and cannot be
used by Void.

Apple requires Developer ID distribution signing, hardened runtime, a secure
timestamp, and notarization for software distributed outside the Mac App Store.
The current `notarytool` workflow supersedes the retired `altool`.

## Constraints

- A branch push, pull request, or schedule must never publish a release.
- The installed artifact must pass normal macOS Gatekeeper checks.
- Network, disk-image, and file-copy work must not block GPUI's foreground
  executor.
- An incomplete release must not become the live update feed.
- Development builds must not overwrite themselves.
- The first implementation must not introduce speculative prerelease channels
  or unsupported platform installers.

## Options considered

1. Copy Zed's release service and channel system. This preserves its feed shape
   but requires unrelated server infrastructure and multiple product channels.
2. Use a third-party application updater. This adds a new runtime and packaging
   architecture where the required stable flow is small and Zed already
   provides a directly relevant lifecycle.
3. Use the latest published GitHub Release as Void's stable feed and adapt
   Zed's macOS installation lifecycle. This keeps publishing and feed
   availability atomic and requires no separate service.

## Decision

Use option 3.

The application bundle identifier is `com.void.desktop`. Only pushed tags
matching `v*.*.*` start `.github/workflows/release.yml`. The tag version must
match `crates/void/Cargo.toml`. CI runs formatting, checking, Clippy, and tests
before packaging. `script/bundle-mac` builds the Apple-silicon app with
`cargo-bundle`, Developer ID signs the app and DMG, notarizes and staples the
DMG, and emits a checksum. The workflow publishes those artifacts only after
all prior steps succeed.

The application icon follows Zed's static macOS bundle pattern. A prepared
512×512 PNG and its 1024×1024 `@2x` counterpart live under
`crates/void/resources/` and are declared in Cargo's bundle metadata.
`cargo-bundle` combines them into `Void.icns`, copies it into the application
resources, and writes `CFBundleIconFile` into `Info.plist`. The source mark is
solid black; the prepared PNGs place it within a centered 824×824 rounded
rectangle with a 100 px transparent inset and a 185 px corner radius on the
1024 px canvas. Keeping this geometry explicit makes future icon exports
reproducible without adding a runtime or build-time image dependency.

Release builds poll
`https://github.com/usamaasfar/void/releases/latest/download/update.json`.
The manifest contains only a stable version, an exact GitHub release-asset URL,
and a lowercase SHA-256 digest. This avoids GitHub REST quotas and keeps the
response contract small.

The updater is implemented in the focused `void_update` crate. Its private `updater`, `manifest`, `download`, `macos`, and `status_view` modules separate task ownership, validation, I/O, platform installation, and rendering while exposing only `Updater` to the binary composition root.

The updater uses GPUI's injected HTTP client and asynchronously streams the DMG
to a temporary installer directory while hashing it. It rejects a checksum
mismatch before mounting. It then validates the DMG with `codesign` and
Gatekeeper and validates the mounted app's nested signatures, Apple generic
anchor, compiled Team ID, identifier, version, and arm64 architecture. The
replacement and mount ownership adapt Zed's `InstallerDir`,
`MacOsUnmounter`, and `rsync --delete` sequence. One owned task controls each
attempt; replacement cancels prior work, and the unmounter's `Drop` schedules
forced detach if cancellation occurs while mounted.

## Intentional differences from Zed

- Void has one stable identity rather than Zed's stable, preview, nightly, and
  development identities.
- GitHub's latest-release asset redirect replaces Zed's private release
  endpoint; Void additionally authenticates the manifest checksum and Apple
  signing identity.
- Void ships only the app DMG; Zed's remote-server assets and other platform
  installers are out of scope.
- Void retries hourly but does not yet expose manual update commands or release
  notes because those interactions were not requested.

## Consequences

Publishing a GitHub Release makes it immediately discoverable by installed
clients. A failed build, signature, or notarization cannot update the feed
because release creation is last. Maintainers must bump the Cargo version
before tagging and must keep the release asset name stable.

The updater depends on macOS-provided `codesign`, `spctl`, `hdiutil`, `lipo`,
`PlistBuddy`, and `rsync`; network I/O no longer depends on `curl`.
Adding Intel macOS, Linux, Windows, prerelease channels, or a manual update
surface requires a separate product decision and platform-specific validation.

## References

- Local Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:
  - `crates/zed/Cargo.toml` bundle identifiers and channel-specific icon lists
  - `crates/zed/resources/app-icon.png`
  - `crates/zed/resources/app-icon@2x.png`
  - `crates/auto_update/src/auto_update.rs::AutoUpdater`
  - `crates/auto_update/src/auto_update.rs::{AutoUpdater::start_polling,AutoUpdater::poll,AutoUpdater::update}`
  - `crates/auto_update/src/auto_update.rs::{InstallerDir,MacOsUnmounter,install_release_macos,cleanup_stale_installer_dirs}`
  - `crates/auto_update_ui/src/auto_update_ui.rs`
  - `script/bundle-mac`
  - `.github/workflows/release.yml`
- Local Sunware commit `ae562ceff699152c15558ee062f293acacd2de35`:
  - `.github/workflows/release-tagged.yml`
  - `apps/desktop/electron-builder.config.ts`
  - `apps/desktop/src/main/updater.ts`
- Apple Developer documentation:
  - <https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution>
  - <https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac>
  - <https://developer.apple.com/design/human-interface-guidelines/app-icons>
  - <https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleiconfile>
- `cargo-bundle` 0.11 metadata documentation:
  - <https://docs.rs/crate/cargo-bundle/0.11.0/source/Readme.md>
- GitHub Actions and Releases documentation:
  - <https://docs.github.com/actions>
  - <https://docs.github.com/repositories/releasing-projects-on-github>
- GPUI documentation: <https://gpui.rs/>

`cargo-bundle` is MIT licensed. The workflow actions used here retain their
upstream licenses and are referenced by immutable commit where adopted from
Zed. Void's updater is an adaptation of the documented Zed lifecycle rather
than a copied Zed crate.
