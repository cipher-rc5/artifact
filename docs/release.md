# Release Requirements

Production release artifacts (`v*.*.*` tags) include:

- Native CI checks on macOS, Linux, and Windows (`cargo test --all-targets --locked`).
- Packaged binaries for supported targets:
  - macOS: a signed + notarized `ARTIFACT.app` bundle distributed as `ARTIFACT-macos-universal.dmg`.
  - Linux x86_64 and aarch64 tarballs (`glibc` 2.17 floor).
  - Windows x86_64: an Authenticode-signed `.exe`, distributed as `artifact-windows-x86_64.zip`.
- `SHA256SUMS` covering every uploaded artifact.
- An SBOM artifact (`cargo metadata` inventory).
- GitHub artifact attestation / build provenance.

The release toolchain is pinned to Rust `1.96` (`dtolnay/rust-toolchain@1.96`) so
releases are reproducible.

## macOS signing & notarization

`.github/workflows/release.yml` runs `scripts/macos-bundle.sh`, which:

1. Builds an `ARTIFACT.app` bundle from the universal binary, generating an
   `Info.plist` (`CFBundleIdentifier = com.cipher.artifact`, `CFBundleName =
   ARTIFACT`, version derived from the release tag).
2. Imports the Developer ID certificate into a temporary keychain and
   `codesign`s the bundle with the **hardened runtime** (`--options runtime`,
   `--timestamp`).
3. Packages the bundle into `ARTIFACT-macos-universal.dmg` and signs the `.dmg`.
4. Notarizes it with `xcrun notarytool submit --wait`.
5. Staples the ticket with `xcrun stapler staple` and validates it.

### Required macOS secrets

| Secret                         | Purpose                                                       |
| ------------------------------ | ------------------------------------------------------------- |
| `APPLE_CERTIFICATE_P12_BASE64` | base64-encoded Developer ID Application `.p12`                |
| `APPLE_CERTIFICATE_PASSWORD`   | password for that `.p12`                                      |
| `APPLE_CODESIGN_IDENTITY`      | signing identity, e.g. `Developer ID Application: … (TEAMID)` |
| `APPLE_ID`                     | Apple ID used for notarization                                |
| `APPLE_TEAM_ID`                | Apple Developer Team ID                                       |
| `APPLE_APP_PASSWORD`           | app-specific password for `notarytool`                        |

## Windows Authenticode signing

`.github/workflows/release.yml` builds the Windows executable on a
`windows-latest` runner and signs it with `scripts/windows-sign.ps1`, which runs
`signtool sign` (SHA-256 file digest, RFC-3161 timestamp via
`http://timestamp.digicert.com`) and then `signtool verify /pa`.

### Required Windows secrets

| Secret                           | Purpose                            |
| -------------------------------- | ---------------------------------- |
| `WINDOWS_CERTIFICATE_PFX_BASE64` | base64-encoded code-signing `.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD`   | password for that `.pfx`           |

## Fail-closed policy

Signing credentials are intentionally not stored in the repository. Because the
release workflow only runs on `v*.*.*` tags, every run is an official release.
The workflow therefore **fails closed**: if any required signing/notarization
secret is absent, a dedicated guard step errors out and the job fails, rather
than shipping an unsigned or un-notarized artifact.
