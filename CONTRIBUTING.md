# Contributing to Limen

Thank you for your interest in contributing to Limen! We welcome contributions, bug reports, and suggestions from developers of all backgrounds.

This guide outlines our development workflow, standards, and submission guidelines.

---

## Code of Conduct

We are committed to providing a welcoming, inclusive, and harassment-free environment for everyone. Please be respectful, constructive, and collaborative in all discussions and interactions.

---

## How Can I Contribute?

- **Reporting Bugs**: Open an issue describing the bug, steps to reproduce, your macOS version, and any relevant logs.
- **Suggesting Features**: Open an issue outlining your proposal, use case, and suggested API or UI design.
- **Submitting Pull Requests**: Implement bug fixes, performance improvements, documentation enhancements, or new capabilities.

---

## Development Workflow

### 1. Environment Requirements
- **OS**: macOS 12 (Monterey) or later
- **Rust**: Latest stable Rust toolchain via `rustup` (`cargo`, `rustc`)
- **Node.js**: v18+ with `npm`
- **Xcode Command Line Tools**: `xcode-select --install`

### 2. Fork and Clone
```bash
git clone https://github.com/<your-username>/limen.git
cd limen
git checkout -b feature/my-improvement
```

### 3. Install Dependencies
```bash
npm install
```

### 4. Running Locally
- **With Live AWS SSO**:
  ```bash
  npm run tauri dev
  ```
- **With Offline Mock Simulator** (no AWS account required):
  ```bash
  LIMEN_MOCK_AUTH=1 npm run tauri dev
  ```

---

## Code Standards & Style

### Rust Backend (`src-tauri/`)
- Follow idiomatic Rust practices and conventions.
- Format code using `cargo fmt`:
  ```bash
  cargo fmt --all --manifest-path src-tauri/Cargo.toml
  ```
- Verify zero compiler errors or warnings:
  ```bash
  cargo check --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml
  ```

### Svelte / TypeScript Frontend (`src/`)
- Adhere to Svelte 5 rune syntax (`$state`, `$derived`, `$props`).
- Run typechecking before submitting:
  ```bash
  npm run check
  ```
- Keep CSS modular and semantic in `src/app.css`. Maintain high-contrast, accessible dark theme tokens.

### Manifest Version Parity
When preparing a version bump or release, keep all project manifests synchronized with identical version numbers:
- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

---

## Continuous Integration & Quality Checks

All Pull Requests trigger automated GitHub Actions CI:
- **Manifest Version Parity**: Ensures all manifest files declare identical versions.
- **Frontend Check**: Runs `npm run check` (`svelte-check`).
- **Backend Check**: Runs `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` on macOS.

### Optional: Local Pre-Commit Hook
If you prefer running fast checks (secret scanning, version parity, and typechecks) automatically before each commit, you can enable the repository's lightweight hook:
```bash
git config core.hooksPath .githooks
```

---

## Submitting a Pull Request

1. Ensure all code builds cleanly and passes type checking:
   ```bash
   npm run check && npm run build && cargo check --manifest-path src-tauri/Cargo.toml
   ```
2. Commit your changes with concise, imperative commit messages (e.g. `feat: add profile expiration timer`, `fix: handle tray right-click menu event`).
3. Push your branch to GitHub and open a Pull Request against `main`.
4. Fill out the PR description with:
   - What changed and why.
   - Any manual or automated verification performed.
   - References to any related issue numbers (e.g. `Fixes #12`).

---

## Release Process (Maintainers)

Limen uses automated, tag-triggered GitHub Actions releases:

1. **Verify Manifest Parity**: Ensure `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` share the target version string.
2. **Tag the Release**:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. **Automated Deployment**:
   - The `.github/workflows/release.yml` workflow compiles the native macOS DMG on Apple Silicon runners (`macos-14`).
   - Calculates SHA-256 checksums (`SHA256SUMS.txt`).
   - Publishes the GitHub Release with downloadable assets and automated release notes.
4. **Update Homebrew Cask**:
   - Copy the generated SHA-256 hash from `SHA256SUMS.txt`.
   - Update `Casks/limen.rb` in your `homebrew-tap` repository with the new version and sha256.

---

## License

By contributing to Limen, you agree that your contributions will be licensed under the project's [MIT License](LICENSE).
