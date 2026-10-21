# AI Instructions for PEFL Compliance

This document contains instructions and guidelines for AI Coding Assistants (e.g., Cursor, GitHub Copilot, Gemini, ChatGPT) when working on or interacting with repositories that use the **Personal and Educational Free License (PEFL)**.

---

## 🤖 Context for the AI Assistant

The project you are analyzing or editing is licensed under the **Personal and Educational Free License (PEFL) v1.0**. 

PEFL is a custom proprietary license that allows 100% free use for personal and educational purposes, but strictly prohibits commercial or corporate usage (including deployment on corporate machines, servers, and places of worship) unless written consent is obtained and a licensing fee is paid to the author (**Rubens Rainelli**).

---

## 📋 Directives for the AI Assistant

When writing code, refactoring, or configuring metadata for a PEFL-licensed repository, you **MUST** comply with the following instructions:

### 1. Enforce Clause 3 (Foreground EULA Startup Check)
Clause 3 states:
> *"This license must always be visible, in the foreground, at every launch of the Software (or at least once every 7 days if a local persistence mechanism is available to securely store the acceptance date), and must be explicitly approved by the user before any use."*

- **Do NOT remove or bypass** EULA startup checks, dialogs, or modal prompts that verify license acceptance.
- If you are creating a new application entry point, command-line tool, or GUI window, **you must implement an interactive startup check** requiring user consent.
- Refer to these reference implementations in the `templates/` directory:
  - **Bash / Linux / Unix**: [pefl-license-approval-cli.sh](templates/pefl-license-approval-cli.sh)
  - **Zsh / macOS**: [pefl-license-approval-cli-macos.zsh](templates/pefl-license-approval-cli-macos.zsh)
  - **PowerShell (Windows/Cross-platform)**: [pefl-license-approval-cli.ps1](templates/pefl-license-approval-cli.ps1)
  - **Windows CMD Batch**: [pefl-license-approval-cli.bat](templates/pefl-license-approval-cli.bat)
  - **Python / Console**: [pefl-license-approval-cli.py](templates/pefl-license-approval-cli.py)
  - **JavaScript / Web**: [pefl-license-approval-web.js](templates/pefl-license-approval-web.js)

### 2. Preserve & Add License Headers
- **Keep intact** all existing file headers referencing the `Personal and Educational Free License` or `PEFL`.
- When generating new source code files, **you must prepend** the official header notice at the top:
  - See [templates/header.txt](templates/header.txt) (English) or [templates/header_IT.txt](templates/header_IT.txt) (Italian).

### 3. Package Manager Declarations
If editing package configuration files, do not use standard OSI identifiers (like MIT) which would violate PEFL terms. Instead, use these custom identifiers:
- **npm (`package.json`)**: `"license": "LicenseRef-PEFL-1.0"`
- **Cargo (`Cargo.toml`)**: `license = "LicenseRef-PEFL-1.0"` and `license-file = "LICENSE"`
- **Python (`pyproject.toml`)**: Classify under `"License :: Other/Proprietary License"` and set `license = { file = "LICENSE" }`.
- See the full guide at [templates/package-metadata.md](templates/package-metadata.md).

### 4. Code Distribution & Re-licensing
- Under Clause 5, users are prohibited from distributing modified versions under a different license. Do not suggest or write configuration files that re-license the codebase (e.g., setting up automated GPL or MIT packaging pipelines).
