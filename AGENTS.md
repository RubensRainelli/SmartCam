# AI Agent Directives & Project Rules (SmartCam)

This file (`AGENTS.md`) is automatically loaded into context by AI coding assistants (Antigravity, Gemini, Cursor, GitHub Copilot, Claude Code, OpenAI Codex, etc.). You **MUST** follow all directives below at all times when working on this repository.

---

## 1. Release, Testing & Versioning Policy (MANDATORY)

### Pre-Release Testing & Manual Verification Gate
- **Prima di pubblicare una release, deve essere sempre testato il funzionamento delle parti modificate.**
- Before publishing, packaging, or tagging any release, you **MUST** test and verify the functionality of all modified components (`cargo test --workspace`, `cargo build --release`, and runtime/functional verification of the affected parts). Never publish a release with untested changes.
- **Local Binary Synchronization**: Always keep `app/target/release/SmartCam.exe` and `bin/smartcam_mf_source.dll` synchronized with `target/release/` after building locally so local test scripts (`start_smartcam.ps1`) always launch the latest build.
- **Acoustic Notification on Manual Test Gate**: When requested by the user (especially before mobile or UI release sign-off), play a completion sound (`[Console]::Beep` / `SystemSounds`) once local refactoring and automated tests finish so the user can manually verify the app before any GitHub release or WinGet publication.

### Multi-Agent Parallel Optimization Workflow
- When splitting optimization or refactoring work across multiple subagents (e.g., Desktop App agent + Mobile Plugin agent):
  1. Assign **strictly disjoint file sets** to each subagent (e.g., `app/src/plugins.rs` isolated from `crates/*` and `app/src/gui.rs`) and keep cross-module public signatures stable.
  2. Have subagents avoid concurrent `cargo build` / `cargo test` invocations while editing to prevent Cargo lock contention on `target/`; run unified verification (`cargo test --workspace && cargo build --release`) in the main coordinator agent once all subagents complete.

### Versioning Standard (`v. A.B.C`) & Release Checklist
Alla pubblicazione di una release va sempre incrementata la versione seguendo lo standard **`v. A.B.C`**:
- **`A` (Main Release / Major)**: Incrementa quando viene richiesto per modifiche sostanziali al progetto (es. passaggio da `v1.0.0` a `v2.0.0` per via del passaggio da C++ a Rust). Quando `A` incrementa, `B` e `C` tornano a `0`.
- **`B` (Feature Release / Minor)**: Versioni intermedie per aggiunte di feature o ottimizzazioni sostanziali (es. `v2.1.0`). Quando `B` incrementa, `C` torna a `0`.
- **`C` (Bugfix Release / Patch)**: Correzioni di bug (bugfix) e fix minori senza aggiunta di nuove funzionalità.

**Files to update on every version bump (`vA.B.C`)**:
1. `Cargo.toml` (`[workspace.package] version = "A.B.C"`) & `Cargo.lock`
2. `app/smartcam.rc` & `app/build.rs` (`FileVersion` and `ProductVersion` `"A.B.C.0"`)
3. `app/src/plugins.rs` & `app/src/linux_app.rs` (built-in version strings)
4. `platforms/windows/installer/smartcam.iss` (`MyAppVersion` and `OutputBaseFilename=SmartCam-Setup-vA.B.C-x64`)
5. `.github/workflows/release.yml` (`SMARTCAM_VERSION: "A.B.C"` and artifact filenames)
6. `CHANGELOG.md`, `README.md`, `README.it.md`, `docs/index.html`, `docs/ARCHITECTURE.md`
7. **WinGet (`microsoft/winget-pkgs`)**: After the signed installer `SmartCam-Setup-vA.B.C-x64.exe` is built and uploaded to the GitHub Release `vA.B.C`, compute its exact `SHA256` and publish/update the WinGet manifests under `manifests/r/RubensRainelli/SmartCam/A.B.C/` via a PR to `microsoft/winget-pkgs`.

---

## 2. PEFL v1.0 Licensing Compliance

@[AI_INSTRUCTIONS.md](AI_INSTRUCTIONS.md)
