//! Authenticated macOS bundle verification, replacement, and mount cleanup.

use std::{
    ffi::OsString,
    fs, mem,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, SystemTime},
};

use anyhow::{Context as _, Result, ensure};
use gpui::BackgroundExecutor;
use semver::Version;
use util::command::new_command;

const INSTALLER_DIR_PREFIX: &str = "void-auto-update";
const STALE_INSTALLER_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const APP_IDENTIFIER: &str = "com.void.desktop";

pub(super) struct InstallerDir(tempfile::TempDir);

impl InstallerDir {
    pub(super) fn new() -> Result<Self> {
        Ok(Self(
            tempfile::Builder::new()
                .prefix(INSTALLER_DIR_PREFIX)
                .tempdir()?,
        ))
    }

    pub(super) fn path(&self) -> &Path {
        self.0.path()
    }
}

struct MacOsUnmounter<'a> {
    mount_path: PathBuf,
    background_executor: &'a BackgroundExecutor,
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

pub(super) async fn install_release_macos(
    _installer: InstallerDir,
    dmg: PathBuf,
    running_app: PathBuf,
    version: Version,
    team_id: String,
    executor: BackgroundExecutor,
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

pub(super) async fn cleanup_stale_installer_dirs() {
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
