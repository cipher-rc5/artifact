# Release Requirements

Production release artifacts must include:

- Native CI checks on macOS, Linux, and Windows.
- Packaged binaries for supported targets.
- `SHA256SUMS` covering every uploaded artifact.
- SBOM artifact.
- GitHub artifact attestation/provenance.
- macOS signing and notarization when Apple credentials are configured.
- Windows Authenticode signing when signing credentials are configured.

Signing credentials are intentionally not stored in the repository. Release workflow steps are guarded by secrets and should fail closed for official production releases.
