# Set up and publish a macOS release

This guide is the complete maintainer checklist for Void's stable macOS
release. Complete the one-time Apple and GitHub setup first. For each later
release, follow only [Publish a release](#publish-a-release).

Void currently publishes only Apple-silicon macOS 12+ releases. Do not create a
GitHub Release manually: pushing a matching stable tag is the sole release
trigger.

## Before you begin

You need:

- membership in the Apple Developer Program;
- access to the Apple Developer team that will publish Void;
- permission to manage certificates and App Store Connect API keys;
- administrator access to the `usamaasfar/void` GitHub repository;
- a Mac with Keychain Access for exporting the signing identity.

Use the same Apple team for the Developer ID certificate, App Store Connect API
key, and `APPLE_TEAM_ID` repository variable. Release builds compile that Team
ID into the updater and reject future updates signed by another team.

## 1. Find the Apple Team ID

Find the publishing team's 10-character Team ID in the Apple Developer account
membership details. It contains only uppercase ASCII letters and digits.

Keep this value available for the GitHub `APPLE_TEAM_ID` variable below.

## 2. Create and export the Developer ID identity

Create a **Developer ID Application** certificate for the publishing team.
Install the certificate on the Mac that generated its private key.

In Keychain Access:

1. Open **My Certificates**.
2. Find the **Developer ID Application** certificate and expand it.
3. Confirm that a private key appears beneath the certificate.
4. Export the certificate and private key together as a `.p12` file.
5. Set a strong export password. This becomes
   `APPLE_CERTIFICATE_PASSWORD`.

Find the exact signing identity with:

```sh
security find-identity -v -p codesigning
```

Copy the complete Developer ID Application identity shown between quotation
marks. It normally has this shape:

```text
Developer ID Application: Organization Name (TEAMID1234)
```

This complete value becomes `APPLE_SIGNING_IDENTITY`. Confirm that the Team ID
in parentheses matches the Team ID from step 1.

Base64-encode the exported `.p12` as a single line:

```sh
base64 < /absolute/path/to/void-release.p12 | tr -d '\n' | pbcopy
```

The clipboard contents become `APPLE_CERTIFICATE_P12_BASE64`. Do not commit the
`.p12` file or its encoded contents. After the GitHub secret has been saved,
store or delete the local export according to the team's credential policy.

## 3. Create an App Store Connect API key

Create a team App Store Connect API key with sufficient access to submit
Developer ID software for notarization. Record:

- the downloaded `.p8` private key;
- the key ID;
- the issuer ID.

Apple permits the private key to be downloaded only once. Store it securely.
The complete textual contents of the `.p8` file, including its
`BEGIN PRIVATE KEY` and `END PRIVATE KEY` lines, become `APPLE_API_KEY`.
The other two values become `APPLE_API_KEY_ID` and
`APPLE_API_ISSUER_ID`.

## 4. Configure the GitHub repository

Open the repository's **Settings → Secrets and variables → Actions** page.

Under **Variables**, create:

| Name | Value |
| --- | --- |
| `APPLE_TEAM_ID` | Exact 10-character Apple Team ID |

This variable is intentionally not secret. The workflow exposes it to the Rust
build as `VOID_UPDATE_SIGNING_TEAM_ID`.

Under **Secrets**, create:

| Name | Value |
| --- | --- |
| `APPLE_CERTIFICATE_P12_BASE64` | Single-line base64 encoding of the `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `KEYCHAIN_PASSWORD` | A strong password used only for the temporary CI keychain |
| `APPLE_SIGNING_IDENTITY` | Complete Developer ID Application identity |
| `APPLE_API_KEY` | Complete contents of the App Store Connect `.p8` key |
| `APPLE_API_KEY_ID` | App Store Connect API key ID |
| `APPLE_API_ISSUER_ID` | App Store Connect API issuer ID |

Secret values must not contain shell quotes added around the actual value.
Preserve the `.p8` key's original line breaks.

## 5. Check the repository configuration

The checked-in release configuration must retain these values:

- bundle identifier: `com.void.desktop`;
- target: `aarch64-apple-darwin`;
- minimum macOS version: `12.0`;
- DMG name: `Void-aarch64.dmg`;
- feed:
  `https://github.com/usamaasfar/void/releases/latest/download/update.json`.

Changing any of these is a product or compatibility decision, not part of a
routine release.

## Publish a release

### 1. Choose and commit a stable version

Set `version` in `crates/void/Cargo.toml` to a stable
`MAJOR.MINOR.PATCH` value. Prerelease suffixes and build metadata are not
accepted by the stable updater.

Run the repository checks:

```sh
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Commit and push the version change before creating the tag. Do not tag
uncommitted work.

### 2. Create and push the exact matching tag

For package version `0.1.0`:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The tag must be exactly `vMAJOR.MINOR.PATCH` and its version must equal the
`void` package version. Branch pushes, pull requests, schedules, and malformed
tags do not publish a release.

### 3. Watch the release workflow

Open the repository's **Actions** page and select the workflow run for the tag.
The workflow must complete all of these gates:

1. validate the tag and Cargo version on an arm64 runner;
2. run formatting, checking, strict Clippy, and all tests;
3. build the macOS 12 arm64 app;
4. import the Developer ID identity into a temporary keychain;
5. sign the executable and then the outer app;
6. strictly verify the complete app bundle;
7. create and sign the DMG;
8. submit the DMG to Apple and wait for notarization;
9. staple and validate the notarization ticket;
10. verify the final DMG and assess it with Gatekeeper;
11. generate the checksum and update manifest;
12. publish the GitHub Release.

The temporary certificate file, notarization key file, staging directory, and
keychain are removed by the workflow.

### 4. Verify the published release

The GitHub Release must contain exactly these update artifacts:

- `Void-aarch64.dmg`;
- `Void-aarch64.dmg.sha256`;
- `update.json`.

Open the feed URL and confirm it reports the new version:

```text
https://github.com/usamaasfar/void/releases/latest/download/update.json
```

Its contract is:

```json
{
  "version": "0.1.0",
  "url": "https://github.com/usamaasfar/void/releases/download/v0.1.0/Void-aarch64.dmg",
  "sha256": "<64 lowercase hexadecimal characters>"
}
```

Download the DMG on an Apple-silicon Mac, open it, drag Void to Applications,
and launch it normally. macOS should accept the app without bypassing
Gatekeeper.

## Troubleshooting

### The tag validation step fails

Confirm that the tag is stable `vMAJOR.MINOR.PATCH` syntax and exactly matches
the package version:

```sh
cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "void") | .version'
git describe --tags --exact-match
```

Delete and recreate a bad local tag only before it has been published. Do not
move or replace a published release tag.

### The certificate import or signing step fails

Check that:

- the `.p12` contains both certificate and private key;
- `APPLE_CERTIFICATE_PASSWORD` is the `.p12` export password;
- `APPLE_CERTIFICATE_P12_BASE64` has no spaces or accidental surrounding
  quotes;
- `APPLE_SIGNING_IDENTITY` exactly matches `security find-identity`;
- the certificate is valid and belongs to `APPLE_TEAM_ID`.

### The release build reports a missing Team ID

Create the repository variable under **Variables**, not **Secrets**, using the
exact name `APPLE_TEAM_ID`. It must be 10 uppercase letters or digits.

### Notarization fails

Check that the `.p8` contents, key ID, and issuer ID belong together and that
the key has permission to submit notarizations for the publishing team. The
workflow waits for Apple's result, so inspect the `notarytool` output in the
failed run before rotating credentials or retrying.

### The workflow succeeds but no release appears

The final publication step requires the workflow's `contents: write`
permission. Confirm that repository or organization Actions policy has not
overridden the workflow token to read-only, then inspect the final
`gh release create` step.

### An update is rejected by an installed copy

Do not bypass the updater checks. Confirm that the release:

- uses a newer stable version;
- retains `com.void.desktop`;
- is signed by the compiled `APPLE_TEAM_ID`;
- contains only the arm64 `Void.app`;
- has a checksum matching `update.json`.

Correct the release pipeline or publish a newer fixed version. Never replace
assets beneath an already published version.

## Authoritative references

- [Release workflow](../../.github/workflows/release.yml)
- [macOS bundle script](../../script/bundle-mac)
- [Release and updater decision](../decisions/0003-tagged-macos-releases-and-updates.md)
- [Apple: Signing Mac software with Developer ID](https://developer.apple.com/developer-id/)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [GitHub: Using secrets in GitHub Actions](https://docs.github.com/actions/security-guides/using-secrets-in-github-actions)
- [GitHub: Managing releases](https://docs.github.com/repositories/releasing-projects-on-github/managing-releases-in-a-repository)
