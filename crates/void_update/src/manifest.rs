//! Stable release-feed parsing, validation, and version selection.

use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use gpui::http_client::{AsyncBody, HttpClient, HttpRequestExt, RedirectPolicy, Request};
use semver::Version;
use serde::Deserialize;
use smol::io::AsyncReadExt;

pub(super) const RELEASE_ASSET_NAME: &str = "Void-aarch64.dmg";
const UPDATE_FEED_URL: &str =
    "https://github.com/usamaasfar/void/releases/latest/download/update.json";
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct ReleaseManifest {
    pub(super) version: String,
    pub(super) url: String,
    pub(super) sha256: String,
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

pub(super) fn newer_stable_version(current: &Version, fetched: &str) -> Result<Option<Version>> {
    let fetched = Version::parse(fetched).context("manifest version is not SemVer")?;
    ensure!(
        fetched.pre.is_empty() && fetched.build.is_empty(),
        "manifest version must be a stable SemVer"
    );
    Ok((fetched > *current).then_some(fetched))
}

pub(super) async fn fetch_manifest(client: Arc<dyn HttpClient>) -> Result<ReleaseManifest> {
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

pub(super) fn verify_checksum(expected: &str, actual: &str) -> Result<()> {
    validate_sha256(expected)?;
    ensure!(
        expected == actual,
        "downloaded DMG SHA-256 does not match the release manifest"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn exact_stable_manifest_contract_is_accepted() {
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
    fn equal_version_is_not_newer() {
        let current = Version::new(2, 0, 0);

        assert_eq!(newer_stable_version(&current, "2.0.0").unwrap(), None);
    }

    #[test]
    fn older_version_is_not_newer() {
        let current = Version::new(2, 0, 0);

        assert_eq!(newer_stable_version(&current, "1.9.9").unwrap(), None);
    }

    #[test]
    fn prerelease_version_is_rejected() {
        let current = Version::new(2, 0, 0);

        assert!(newer_stable_version(&current, "2.1.0-beta.1").is_err());
    }

    #[test]
    fn newer_stable_version_is_returned() {
        let current = Version::new(2, 0, 0);

        assert_eq!(
            newer_stable_version(&current, "2.1.0").unwrap(),
            Some(Version::new(2, 1, 0))
        );
    }

    #[test]
    fn lowercase_sha256_is_accepted() {
        assert!(validate_sha256(HASH).is_ok());
    }

    #[test]
    fn uppercase_sha256_is_rejected() {
        assert!(validate_sha256(&HASH.to_uppercase()).is_err());
    }

    #[test]
    fn short_sha256_is_rejected() {
        assert!(validate_sha256("abcd").is_err());
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        assert!(verify_checksum(HASH, &"f".repeat(64)).is_err());
    }
}
