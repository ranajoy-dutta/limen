# Security Policy

Limen takes the security of credentials, authentication tokens, and the software supply chain seriously. This document outlines our security architecture, vulnerability reporting procedures, and release verification standards.

---

## Supported Versions

Only the latest published release receives active security patches.

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

---

## Reporting a Vulnerability

If you discover a security vulnerability or potential credential leak in Limen, please **do not open a public GitHub issue**.

### Preferred Method: GitHub Private Vulnerability Reporting
1. Navigate to the [Limen Security Advisories](https://github.com/ranajoy-dutta/limen/security/advisories) page.
2. Click **"Report a vulnerability"** to submit your findings privately.
3. Include:
   - A clear description of the vulnerability.
   - Steps or proof-of-concept to reproduce the behavior.
   - Any potential impact on user credentials or token secrecy.

### Response Commitment
- **Initial Acknowledgment**: Within 48 hours of receipt.
- **Assessment & Patch Timeline**: Critical vulnerabilities will be investigated immediately, with a patch released via a GitHub security advisory and patch version bump.

---

## Security Architecture & Design

### 1. POSIX File Permission Isolation
- **Credentials File (`~/.aws/credentials`)**: Written atomically using temporary files and renamed in-place with strict `0600` (read/write only by the owner) POSIX permissions.
- **Client Registrations**: Cached client metadata is stored in user application caches with `0600` file permissions.
- **AWS Directory (`~/.aws`)**: Enforces `0700` directory permissions upon creation.

### 2. In-Memory Token Lifecycle
- Temporary STS credentials and access tokens are managed in memory within scoped Rust data structures.
- Sensitive values are cleared upon manual logout, session expiration, or application termination.

### 3. Zero Telemetry & Direct AWS Connections
- Limen collects **no analytics, telemetry, or user metrics**.
- Network traffic is strictly isolated to official AWS SSO-OIDC and AWS IAM Identity Center regional endpoints. No third-party servers or intermediary proxies are ever contacted.

### 4. Supply Chain Integrity & Attestations
Every official release `.dmg` is compiled in an isolated GitHub Actions runner and signed with a cryptographic **SLSA Build Attestation**.

You can independently verify the provenance and integrity of any downloaded binary using the GitHub CLI:

```bash
gh attestation verify Limen_0.1.0_aarch64.dmg --owner ranajoy-dutta
```

This mathematically proves that the binary was built directly from the public GitHub source repository without tampering.
