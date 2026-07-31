//! Streaming release download and checksum verification.

use std::{path::Path, sync::Arc};

use anyhow::{Context as _, Result, ensure};
use gpui::http_client::{AsyncBody, HttpClient, HttpRequestExt, RedirectPolicy, Request};
use sha2::{Digest, Sha256};
use smol::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::manifest::{ReleaseManifest, verify_checksum};

pub(super) async fn download_release(
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

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use gpui::http_client::{FakeHttpClient, Response};

    use super::*;

    #[test]
    fn download_streams_verified_bytes_and_reports_progress() {
        smol::block_on(async {
            let client = FakeHttpClient::create(|_| async {
                Ok(Response::builder()
                    .status(200)
                    .header("content-length", "3")
                    .body(b"abc".as_slice().into())
                    .unwrap())
            });
            let manifest = ReleaseManifest {
                version: "1.2.3".to_owned(),
                url: "https://test.example/Void-aarch64.dmg".to_owned(),
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                    .to_owned(),
            };
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("Void-aarch64.dmg");
            let progress = Rc::new(RefCell::new(Vec::new()));

            download_release(client, &manifest, &target, {
                let progress = progress.clone();
                move |value| progress.borrow_mut().push(value)
            })
            .await
            .unwrap();

            assert_eq!(
                (
                    smol::fs::read(target).await.unwrap(),
                    progress.borrow().clone()
                ),
                (b"abc".to_vec(), vec![Some(0.0), Some(1.0)])
            );
        });
    }
}
