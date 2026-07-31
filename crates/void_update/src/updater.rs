//! Update status transitions and owned polling/install task.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result};
use gpui::{AppContext as _, Context, MouseUpEvent, Task, Window, http_client::HttpClient};
use semver::Version;

use crate::{
    download::download_release,
    macos::{InstallerDir, cleanup_stale_installer_dirs, install_release_macos},
    manifest::{RELEASE_ASSET_NAME, fetch_manifest, newer_stable_version},
};

const POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, PartialEq)]
pub(super) enum UpdateStatus {
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

/// GPUI-owned stable-channel updater and status surface.
pub struct Updater {
    current_version: Version,
    running_app_path: PathBuf,
    http_client: Arc<dyn HttpClient>,
    pub(super) status: UpdateStatus,
    task: Option<Task<Result<()>>>,
}

impl Updater {
    /// Creates an updater for the running application bundle.
    ///
    /// Invalid versions and unsupported or non-release builds remain disabled.
    pub fn new(current_version: &str, app_path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let current_version = Version::parse(current_version);
        let running_app_path = app_path.unwrap_or_default();
        let enabled = current_version.is_ok()
            && updater_is_enabled(
                cfg!(target_os = "macos"),
                cfg!(target_arch = "aarch64"),
                option_env!("VOID_RELEASE_BUILD"),
                option_env!("VOID_UPDATE_SIGNING_TEAM_ID").unwrap_or_default(),
                &running_app_path,
            );
        Self {
            current_version: current_version.unwrap_or_else(|_| Version::new(0, 0, 0)),
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

    /// Starts automatic update polling when this build supports self-update.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.status,
            UpdateStatus::Disabled | UpdateStatus::Ready { .. }
        ) {
            self.task = Some(Self::update_task(true, cx));
        }
    }

    pub(super) fn retry(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn restart(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.status, UpdateStatus::Ready { .. }) {
            cx.restart();
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

    const TEAM_ID: &str = "ABCDEFGHIJ";

    #[test]
    fn updater_is_enabled_for_release_bundle_on_apple_silicon() {
        assert!(updater_is_enabled(
            true,
            true,
            Some("1"),
            TEAM_ID,
            Path::new("/Applications/Void.app")
        ));
    }

    #[test]
    fn updater_is_disabled_off_macos() {
        assert!(!updater_is_enabled(
            false,
            true,
            Some("1"),
            TEAM_ID,
            Path::new("/Applications/Void.app")
        ));
    }

    #[test]
    fn updater_is_disabled_off_apple_silicon() {
        assert!(!updater_is_enabled(
            true,
            false,
            Some("1"),
            TEAM_ID,
            Path::new("/Applications/Void.app")
        ));
    }

    #[test]
    fn updater_is_disabled_without_release_marker() {
        assert!(!updater_is_enabled(
            true,
            true,
            None,
            TEAM_ID,
            Path::new("/Applications/Void.app")
        ));
    }

    #[test]
    fn updater_is_disabled_without_team_id() {
        assert!(!updater_is_enabled(
            true,
            true,
            Some("1"),
            "",
            Path::new("/Applications/Void.app")
        ));
    }

    #[test]
    fn updater_is_disabled_for_unbundled_binary() {
        assert!(!updater_is_enabled(
            true,
            true,
            Some("1"),
            TEAM_ID,
            Path::new("/tmp/void")
        ));
    }
}
