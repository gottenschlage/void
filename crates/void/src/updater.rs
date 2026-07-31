//! Authenticated stable-channel updates for signed Apple-silicon app bundles.
//!
//! One owned task controls polling, download, and installation. Mounted-image
//! cleanup has separate ownership so cancellation cannot leak a mount.

use std::{
    ffi::OsString,
    fs, mem,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Context as _, Result, ensure};
use gpui::{
    Context, IntoElement, MouseButton, MouseUpEvent, Render, Task, Window, div,
    http_client::{AsyncBody, HttpClient, HttpRequestExt, RedirectPolicy, Request},
    prelude::*,
    px,
};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use smol::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};
use theme::ActiveTheme;
use util::command::new_command;

const UPDATE_FEED_URL: &str =
    "https://github.com/usamaasfar/void/releases/latest/download/update.json";
const INSTALLER_DIR_PREFIX: &str = "void-auto-update";
const RELEASE_ASSET_NAME: &str = "Void-aarch64.dmg";
const APP_IDENTIFIER: &str = "com.void.desktop";
const POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);
const STALE_INSTALLER_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UpdateStatus {
    Disabled,
    Idle,
    Checking,
    Downloading {
        version: Version,
        progress: Option<f32>,
    },
    Installing {
        version: Version,
    },
    Ready {
        version: Version,
    },
    Errored {
        message: String,
    },
}

pub(crate) struct Updater {
    current_version: Version,
    running_app_path: PathBuf,
    http_client: Arc<dyn HttpClient>,
    status: UpdateStatus,
    task: Option<Task<Result<()>>>,
}

impl Updater {
    pub(crate) fn new(app_path: Result<PathBuf>, cx: &mut Context<Self>) -> Self {
        let current_version = Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("Cargo package version must be semver");
        let running_app_path = app_path.unwrap_or_default();
        let enabled = updater_is_enabled(
            cfg!(target_os = "macos"),
            cfg!(target_arch = "aarch64"),
            option_env!("VOID_RELEASE_BUILD"),
            option_env!("VOID_UPDATE_SIGNING_TEAM_ID").unwrap_or_default(),
            &running_app_path,
        );
        Self {
            current_version,
            running_app_path,
            http_client: cx.http_client(),
            status: if enabled {
                UpdateStatus::Idle
            } else {
                UpdateStatus::Disabled
            },
            task: None,
        }
    }

    pub(crate) fn start(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.status,
            UpdateStatus::Disabled | UpdateStatus::Ready { .. }
        ) {
            self.task = Some(Self::update_task(true, cx));
        }
    }

    fn retry(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.status, UpdateStatus::Errored { .. }) {
            self.task.take();
            self.task = Some(Self::update_task(false, cx));
        }
    }

    fn update_task(automatic: bool, cx: &mut Context<Self>) -> Task<Result<()>> {
        cx.background_spawn(cleanup_stale_installer_dirs()).detach();
        cx.spawn(async move |this, cx| {
            loop {
                let (client, current_version, app_path) = this.read_with(cx, |this, _| {
                    (
                        this.http_client.clone(),
                        this.current_version.clone(),
                        this.running_app_path.clone(),
                    )
                })?;
                this.update(cx, |this, cx| this.set_status(UpdateStatus::Checking, cx))?;
                let manifest = match fetch_manifest(client.clone()).await {
                    Ok(manifest) => manifest,
                    Err(error) if automatic => {
                        eprintln!("automatic update check failed: {error:#}");
                        this.update(cx, |this, cx| this.set_status(UpdateStatus::Idle, cx))?;
                        cx.background_executor().timer(POLL_INTERVAL).await;
                        continue;
                    }
                    Err(error) => {
                        this.update(cx, |this, cx| this.finish_error(error, cx))?;
                        return Ok(());
                    }
                };
                let Some(version) = newer_stable_version(&current_version, &manifest.version)?
                else {
                    this.update(cx, |this, cx| this.set_status(UpdateStatus::Idle, cx))?;
                    cx.background_executor().timer(POLL_INTERVAL).await;
                    continue;
                };
                this.update(cx, |this, cx| {
                    this.set_status(
                        UpdateStatus::Downloading {
                            version: version.clone(),
                            progress: None,
                        },
                        cx,
                    )
                })?;

                let installer = match InstallerDir::new().context("create installer directory") {
                    Ok(installer) => installer,
                    Err(error) => {
                        this.update(cx, |this, cx| this.finish_error(error, cx))?;
                        return Ok(());
                    }
                };
                let dmg = installer.path().join(RELEASE_ASSET_NAME);
                let progress_entity = this.clone();
                let mut progress_cx = cx.clone();
                if let Err(error) = download_release(client, &manifest, &dmg, move |progress| {
                    let _ = progress_entity.update(&mut progress_cx, |this, cx| {
                        if let UpdateStatus::Downloading {
                            progress: current, ..
                        } = &mut this.status
                        {
                            *current = progress;
                            cx.notify();
                        }
                    });
                })
                .await
                {
                    this.update(cx, |this, cx| this.finish_error(error, cx))?;
                    return Ok(());
                }
                this.update(cx, |this, cx| {
                    this.set_status(
                        UpdateStatus::Installing {
                            version: version.clone(),
                        },
                        cx,
                    )
                })?;
                let executor = cx.background_executor().clone();
                let result = cx
                    .background_spawn(install_release_macos(
                        installer,
                        dmg,
                        app_path,
                        version.clone(),
                        option_env!("VOID_UPDATE_SIGNING_TEAM_ID")
                            .unwrap_or_default()
                            .to_owned(),
                        executor,
                    ))
                    .await;
                this.update(cx, |this, cx| match result {
                    Ok(()) => {
                        this.task = None;
                        this.set_status(UpdateStatus::Ready { version }, cx);
                    }
                    Err(error) => this.finish_error(error, cx),
                })?;
                return Ok(());
            }
        })
    }

    fn finish_error(&mut self, error: anyhow::Error, cx: &mut Context<Self>) {
        self.task = None;
        self.set_status(
            UpdateStatus::Errored {
                message: format!("{error:#}"),
            },
            cx,
        );
    }

    fn set_status(&mut self, status: UpdateStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    fn restart(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.status, UpdateStatus::Ready { .. }) {
            cx.restart();
        }
    }
}

impl Render for Updater {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (label, action) = match &self.status {
            UpdateStatus::Downloading { version, progress } => (
                progress.map_or_else(
                    || format!("Downloading Void {version}…"),
                    |value| format!("Downloading Void {version}… {:.0}%", value * 100.0),
                ),
                None,
            ),
            UpdateStatus::Installing { version } => (format!("Installing Void {version}…"), None),
            UpdateStatus::Ready { version } => {
                (format!("Restart to update to {version}"), Some(true))
            }
            UpdateStatus::Errored { message } => {
                (format!("Update failed: {message} — Retry"), Some(false))
            }
            UpdateStatus::Disabled | UpdateStatus::Idle | UpdateStatus::Checking => {
                (String::new(), None)
            }
        };
        if label.is_empty() {
            return div().into_any_element();
        }
        div()
            .id("void-update-status")
            .absolute()
            .right(px(12.))
            .bottom(px(12.))
            .max_w(px(520.))
            .rounded_sm()
            .bg(cx.theme().colors().surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .px_3()
            .py_2()
            .text_xs()
            .when(action == Some(true), |button| {
                button
                    .cursor_pointer()
                    .hover(|button| button.bg(cx.theme().colors().element_hover))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::restart))
            })
            .when(action == Some(false), |button| {
                button
                    .cursor_pointer()
                    .hover(|button| button.bg(cx.theme().colors().element_hover))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::retry))
            })
            .child(label)
            .into_any_element()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct ReleaseManifest {
    version: String,
    url: String,
    sha256: String,
}

fn parse_manifest(body: &[u8]) -> Result<ReleaseManifest> {
    ensure!(
        body.len() <= MAX_MANIFEST_BYTES,
        "update manifest exceeds {MAX_MANIFEST_BYTES} bytes"
    );
    let manifest: ReleaseManifest =
        serde_json::from_slice(body).context("update feed contains invalid JSON")?;
    let version = Version::parse(&manifest.version).context("manifest version is not SemVer")?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "manifest version must be a stable SemVer"
    );
    let expected_url = format!(
        "https://github.com/usamaasfar/void/releases/download/v{version}/{RELEASE_ASSET_NAME}"
    );
    ensure!(
        manifest.url == expected_url,
        "update URL does not match the manifest version and required asset"
    );
    validate_sha256(&manifest.sha256)?;
    Ok(manifest)
}

fn validate_sha256(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "manifest SHA-256 must be 64 lowercase hexadecimal characters"
    );
    Ok(())
}

fn newer_stable_version(current: &Version, fetched: &str) -> Result<Option<Version>> {
    let fetched = Version::parse(fetched).context("manifest version is not SemVer")?;
    ensure!(
        fetched.pre.is_empty() && fetched.build.is_empty(),
        "manifest version must be a stable SemVer"
    );
    Ok((fetched > *current).then_some(fetched))
}

async fn fetch_manifest(client: Arc<dyn HttpClient>) -> Result<ReleaseManifest> {
    let request = Request::builder()
        .uri(UPDATE_FEED_URL)
        .follow_redirects(RedirectPolicy::FollowAll)
        .body(AsyncBody::default())
        .context("build update feed request")?;
    let mut response = client.send(request).await.context("fetch update feed")?;
    ensure!(
        response.status().is_success(),
        "update feed returned HTTP {}",
        response.status()
    );
    let mut body = Vec::new();
    response
        .body_mut()
        .take((MAX_MANIFEST_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .await
        .context("read update feed")?;
    parse_manifest(&body)
}

async fn download_release(
    client: Arc<dyn HttpClient>,
    manifest: &ReleaseManifest,
    target: &Path,
    mut progress: impl FnMut(Option<f32>),
) -> Result<()> {
    let request = Request::builder()
        .uri(&manifest.url)
        .follow_redirects(RedirectPolicy::FollowAll)
        .body(AsyncBody::default())
        .context("build update download request")?;
    let mut response = client.send(request).await.context("download update")?;
    ensure!(
        response.status().is_success(),
        "update download returned HTTP {}",
        response.status()
    );
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    progress(total.map(|_| 0.0));
    let mut file = File::create(target)
        .await
        .context("create downloaded DMG")?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = response
            .body_mut()
            .read(&mut buffer)
            .await
            .context("read downloaded DMG")?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .await
            .context("write downloaded DMG")?;
        hasher.update(&buffer[..count]);
        downloaded += count as u64;
        progress(total.map(|size| (downloaded as f32 / size as f32).min(1.0)));
    }
    file.flush().await.context("flush downloaded DMG")?;
    verify_checksum(&manifest.sha256, &format!("{:x}", hasher.finalize()))
}

fn verify_checksum(expected: &str, actual: &str) -> Result<()> {
    validate_sha256(expected)?;
    ensure!(
        expected == actual,
        "downloaded DMG SHA-256 does not match the release manifest"
    );
    Ok(())
}

struct InstallerDir(tempfile::TempDir);

impl InstallerDir {
    fn new() -> Result<Self> {
        Ok(Self(
            tempfile::Builder::new()
                .prefix(INSTALLER_DIR_PREFIX)
                .tempdir()?,
        ))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

struct MacOsUnmounter<'a> {
    mount_path: PathBuf,
    background_executor: &'a gpui::BackgroundExecutor,
}

impl MacOsUnmounter<'_> {
    async fn unmount(mut self) -> Result<()> {
        let mount_path = mem::take(&mut self.mount_path);
        detach_disk_image(&mount_path).await
    }
}

impl Drop for MacOsUnmounter<'_> {
    fn drop(&mut self) {
        let mount_path = mem::take(&mut self.mount_path);
        if !mount_path.as_os_str().is_empty() {
            self.background_executor
                .spawn(async move {
                    if let Err(error) = detach_disk_image(&mount_path).await {
                        eprintln!("failed to detach cancelled Void update: {error:#}");
                    }
                })
                .detach();
        }
    }
}

async fn detach_disk_image(mount_path: &Path) -> Result<()> {
    checked_async_output(
        new_command("hdiutil")
            .args(["detach", "-force"])
            .arg(mount_path),
        "detach update disk image",
    )
    .await
    .map(drop)
}

async fn install_release_macos(
    _installer: InstallerDir,
    dmg: PathBuf,
    running_app: PathBuf,
    version: Version,
    team_id: String,
    executor: gpui::BackgroundExecutor,
) -> Result<()> {
    ensure!(
        !team_id.is_empty(),
        "updater was compiled without an Apple Team ID"
    );
    run_checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "--verbose=2"])
            .arg(&dmg),
        "verify DMG signature",
    )?;
    run_checked(
        Command::new("/usr/sbin/spctl")
            .args([
                "--assess",
                "--type",
                "open",
                "--context",
                "context:primary-signature",
                "--verbose=2",
            ])
            .arg(&dmg),
        "assess DMG with Gatekeeper",
    )?;
    let mount = dmg
        .parent()
        .context("downloaded DMG has no parent")?
        .join("Void");
    checked_async_output(
        new_command("hdiutil")
            .args(["attach", "-readonly", "-nobrowse", "-mountpoint"])
            .arg(&mount)
            .arg(&dmg),
        "mount update disk image",
    )
    .await?;
    let unmounter = MacOsUnmounter {
        mount_path: mount.clone(),
        background_executor: &executor,
    };
    let mounted_app = require_void_app(&mount)?;
    validate_app_bundle(&mounted_app, &version, &team_id)?;
    let mut source = OsString::from(&mounted_app);
    source.push("/");
    let copy_result = checked_async_output(
        new_command("rsync")
            .args(["-av", "--delete", "--exclude", "Icon?"])
            .arg(source)
            .arg(&running_app),
        "replace the running application",
    )
    .await;
    let detach_result = unmounter.unmount().await;
    copy_result?;
    detach_result
}

fn require_void_app(mount: &Path) -> Result<PathBuf> {
    let apps = fs::read_dir(mount)
        .context("read mounted update")?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .collect::<Vec<_>>();
    ensure!(
        apps.len() == 1 && apps[0].file_name().is_some_and(|name| name == "Void.app"),
        "mounted update must contain exactly Void.app"
    );
    Ok(apps[0].clone())
}

fn validate_app_bundle(app: &Path, version: &Version, team_id: &str) -> Result<()> {
    run_checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict", "--verbose=2"])
            .arg(app),
        "verify nested application signatures",
    )?;
    let requirement = format!(
        "anchor apple generic and identifier \"{APP_IDENTIFIER}\" and certificate leaf[subject.OU] = \"{team_id}\""
    );
    run_checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "-R"])
            .arg(requirement)
            .arg(app),
        "verify application signing requirement",
    )?;
    let plist = app.join("Contents/Info.plist");
    ensure!(
        plist_value(&plist, "CFBundleIdentifier")? == APP_IDENTIFIER,
        "update has the wrong bundle identifier"
    );
    ensure!(
        plist_value(&plist, "CFBundleShortVersionString")? == version.to_string(),
        "update bundle version does not match its manifest"
    );
    let architectures = checked_output(
        Command::new("/usr/bin/lipo")
            .arg("-archs")
            .arg(app.join("Contents/MacOS/void")),
        "inspect update architecture",
    )?;
    ensure!(
        String::from_utf8_lossy(&architectures.stdout)
            .split_whitespace()
            .eq(["arm64"]),
        "update executable must contain only arm64 code"
    );
    Ok(())
}

fn plist_value(plist: &Path, key: &str) -> Result<String> {
    let output = checked_output(
        Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", &format!("Print :{key}")])
            .arg(plist),
        "inspect update bundle metadata",
    )?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn run_checked(command: &mut Command, operation: &str) -> Result<()> {
    checked_output(command, operation).map(drop)
}

fn checked_output(command: &mut Command, operation: &str) -> Result<Output> {
    let output = command
        .output()
        .with_context(|| format!("failed to {operation}"))?;
    ensure!(
        output.status.success(),
        "failed to {operation}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output)
}

async fn checked_async_output(
    command: &mut util::command::Command,
    operation: &str,
) -> Result<Output> {
    let output = command
        .output()
        .await
        .with_context(|| format!("failed to {operation}"))?;
    ensure!(
        output.status.success(),
        "failed to {operation}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output)
}

async fn cleanup_stale_installer_dirs() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(INSTALLER_DIR_PREFIX)
        {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_INSTALLER_AGE);
        if stale {
            let _ = smol::fs::remove_dir_all(path).await;
        }
    }
}

fn updater_is_enabled(
    is_macos: bool,
    is_aarch64: bool,
    release_build: Option<&str>,
    team_id: &str,
    app_path: &Path,
) -> bool {
    is_macos
        && is_aarch64
        && release_build == Some("1")
        && team_id.len() == 10
        && app_path
            .extension()
            .is_some_and(|extension| extension == "app")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_the_exact_stable_manifest_contract() {
        let manifest = parse_manifest(
            format!(
                r#"{{"version":"1.2.3","url":"https://github.com/usamaasfar/void/releases/download/v1.2.3/Void-aarch64.dmg","sha256":"{HASH}"}}"#
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(manifest.version, "1.2.3");
    }

    #[test]
    fn rejects_prerelease_equal_and_older_versions() {
        let current = Version::new(2, 0, 0);
        assert!(newer_stable_version(&current, "2.0.0").unwrap().is_none());
        assert!(newer_stable_version(&current, "1.9.9").unwrap().is_none());
        assert!(newer_stable_version(&current, "2.1.0-beta.1").is_err());
        assert_eq!(
            newer_stable_version(&current, "2.1.0").unwrap(),
            Some(Version::new(2, 1, 0))
        );
    }

    #[test]
    fn validates_strict_lowercase_sha256_and_mismatches() {
        assert!(validate_sha256(HASH).is_ok());
        assert!(validate_sha256(&HASH.to_uppercase()).is_err());
        assert!(validate_sha256("abcd").is_err());
        assert!(verify_checksum(HASH, &"f".repeat(64)).is_err());
    }

    #[test]
    fn release_enablement_requires_every_build_invariant() {
        let app = Path::new("/Applications/Void.app");
        assert!(updater_is_enabled(true, true, Some("1"), "ABCDEFGHIJ", app));
        assert!(!updater_is_enabled(
            false,
            true,
            Some("1"),
            "ABCDEFGHIJ",
            app
        ));
        assert!(!updater_is_enabled(
            true,
            false,
            Some("1"),
            "ABCDEFGHIJ",
            app
        ));
        assert!(!updater_is_enabled(true, true, None, "ABCDEFGHIJ", app));
        assert!(!updater_is_enabled(true, true, Some("1"), "", app));
        assert!(!updater_is_enabled(
            true,
            true,
            Some("1"),
            "ABCDEFGHIJ",
            Path::new("/tmp/void")
        ));
    }
}
