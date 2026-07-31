# ADR 0013: Own update infrastructure in `void_update`

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

The binary crate implemented the complete stable Apple-silicon update lifecycle
in one `updater.rs`: GPUI status and task ownership, feed parsing, download and
hashing, macOS verification and replacement, mount cleanup, and rendering. This
behavior has an independent process and cancellation lifecycle and requires no
workspace domain state.

ADR 0003 already selected Void's release and security contract. This decision
concerns code ownership only; weakening or expanding that contract is out of
scope.

Pinned Zed separates `auto_update` from `auto_update_ui`. Its `AutoUpdater`
retains polling and attempt tasks, performs network and installer work away from
the foreground executor, and relies on `InstallerDir` and `MacOsUnmounter` drop
behavior for cancellation cleanup. Zed's complete implementation also includes
channels, remote servers, settings, release notes, Linux, Windows, and
persistent notifications that Void does not implement.

## Decision

Move the existing updater into one focused `void_update` library crate with
private modules:

- `updater.rs` owns update status transitions and the one active polling,
  download, or installation `Task`.
- `manifest.rs` owns the bounded GitHub feed, exact asset URL, stable SemVer,
  and lowercase SHA-256 contract.
- `download.rs` streams the response to disk, reports progress, hashes the same
  bytes, and rejects checksum mismatches.
- `macos.rs` owns temporary installers, signature and Gatekeeper checks, bundle
  identity/version/architecture validation, read-only mounting, replacement,
  unmounting, and stale-directory cleanup.
- `status_view.rs` renders the existing retry and restart surface.

Only `Updater` is public. The binary passes its own Cargo package version when
constructing it, preserving the release workflow's `void` package-version
contract rather than coupling update selection to the infrastructure crate's
version.

Keep opaque orchestration errors internal as `anyhow::Error`; no caller branches
on an error variant, and the public boundary returns no error. Keep the injected
`Arc<dyn HttpClient>` because GPUI provides the HTTP implementation at runtime.
Do not add a generalized transport or installer trait without another concrete
implementation.

## Lifecycle and safety

`Updater` retains its active task. Replacing or releasing it cancels pending
polling, download, or installation work. Stale-directory cleanup is detached
because it owns only its path scan and does not affect update state.

After a disk image is mounted, `MacOsUnmounter` awaits forced detach on the
normal path. If the installation future is cancelled or exits early, its
`Drop` implementation schedules detach on the retained background executor.
The installer directory remains owned by the installation future and is removed
only after its resources are released.

The extraction does not change the feed URL, asset name, polling interval,
manifest size limit, stable-version policy, checksum, Team ID, bundle ID,
arm64-only requirement, signature checks, Gatekeeper assessment, mount flags,
`rsync` flags, retry behavior, restart behavior, or disabled-build conditions.

## Consequences

- The binary is limited to constructing and composing the updater entity.
- Network, security, platform, state, and rendering responsibilities can be
  reviewed and tested independently.
- Void keeps one crate rather than mirroring Zed's larger core/UI split because
  its only UI is a small status surface and both parts share one entity.
- Linux, Windows, prerelease channels, settings, release notes, and manual
  update commands remain absent.
- No Zed source was copied in this extraction; existing Void code was moved and
  organized using the verified lifecycle boundaries.

## References

Verified against local Zed commit
`5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/auto_update/src/auto_update.rs::{AutoUpdater::start_polling,AutoUpdater::poll,AutoUpdater::update}`
- `crates/auto_update/src/auto_update.rs::{download_release,InstallerDir,MacOsUnmounter,install_release_macos,cleanup_stale_installer_dirs}`
- `crates/auto_update_ui/src/auto_update_ui.rs`
- `crates/gpui/src/app/context.rs::{Context::spawn,AppContext::background_spawn}`
- `crates/gpui/src/executor.rs::{ForegroundExecutor::spawn,BackgroundExecutor::spawn}`

Current primary documentation consulted:

- GPUI: <https://gpui.rs/>
- `tempfile::TempDir`: <https://docs.rs/tempfile/3.27.0/tempfile/struct.TempDir.html>
- `sha2::{Digest,Sha256}`: <https://docs.rs/sha2/0.10.9/sha2/>

ADR 0003 remains authoritative for the release, signing, notarization, feed, and
platform-security decision.
