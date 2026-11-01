# AI Agent Directives & Project Rules (SmartCam)

This file (`AGENTS.md`) is automatically loaded into context by AI coding assistants (Antigravity, Gemini, Cursor, GitHub Copilot, Claude Code, OpenAI Codex, etc.). You **MUST** follow all directives below at all times when working on this repository.

---

## 1. Release, Testing & Versioning Policy (MANDATORY)

### Pre-Release Testing
- **Prima di pubblicare una release, deve essere sempre testato il funzionamento delle parti modificate.**
- Before publishing, packaging, or tagging any release, you **MUST** test and verify the functionality of all modified components (`cargo test`, build checks, and runtime/functional verification of the affected parts). Never publish a release with untested changes.

### Versioning Standard (`v. A.B.C`)
Alla pubblicazione di una release va sempre incrementata la versione seguendo lo standard **`v. A.B.C`** (aggiornando `Cargo.toml`, `CHANGELOG.md` e i metadati di packaging/installer):
- **`A` (Main Release / Major)**: Incrementa quando viene richiesto per modifiche sostanziali al progetto (es. passaggio da `v1.0.0` a `v2.0.0` per via del passaggio da C++ a Rust). Quando `A` incrementa, `B` e `C` tornano a `0`.
- **`B` (Feature Release / Minor)**: Versioni intermedie per aggiunte di feature prima non presenti. Quando `B` incrementa, `C` torna a `0`.
- **`C` (Bugfix Release / Patch)**: Correzioni di bug (bugfix) e fix minori senza aggiunta di nuove funzionalità.

---

## 2. PEFL v1.0 Licensing Compliance

@[AI_INSTRUCTIONS.md](AI_INSTRUCTIONS.md)
