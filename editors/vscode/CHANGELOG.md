# Change Log

## 3.0.9 - 2026-10-22

- Package alongside Pinset 3.0.9 import, generic binary Provider, CI support and command documentation improvements.
- Show complete structured reports when tool checks return a failing status, while retaining errors for invalid configuration and failed processes.

## 3.0.8 - 2026-10-20

- Package alongside the Pinset 3.0.8 symbolic-link CLI maintenance fix; no extension behavior change.

## 3.0.7 - 2026-10-16

- Package alongside the Pinset 3.0.7 global fallback and managed shim refresh fixes.
- No extension behavior change; full extension-host acceptance was not repeated for this focused CLI hotfix.

## 3.0.6 - 2026-10-15

- Keep nested folders from overwriting a parent project's SDK settings and bind each root independently in multi-root workspaces.
- Use the installed global Flutter SDK path when no project-local binding exists.
- Package alongside Pinset 3.0.6 with local extension-host regression coverage.

## 3.0.1 - 2026-10-06

- Package the extension alongside Pinset 3.0.1 and its simplified version-switching commands.
- Keep project status, installation, explicit probes and SDK/JDK binding on protocol 3.

## 3.0.0 - 2026-10-06

- Require the Pinset 3 protocol and a trusted local project.
- Retain five commands: refresh, install, check, probe and SDK binding.
- Bind the project Temurin JDK separately from the Java language server JDK.
- Remove compatibility protocols, environment panels, task providers and CLI installation flows.

## 1.2.0

- Add per-folder environment preparation, readiness and execution evidence.
- Preview and restore Node, Python and Flutter workspace bindings.
- Observe controlled native debugger launches independently of configuration readiness.
- Negotiate protocol 2 with CLI 2.13+ while retaining protocol 1 compatibility. Environment bindings require CLI 2.15+ and are checked against the local host and directory generation before restoration.

## 1.1.2

- Center the Pinset mark on a white extension-icon background for clearer Marketplace and VS Code rendering at small sizes.

## 1.1.1

- Add the Pinset brand icon to the Visual Studio Marketplace package.
- Detect a missing Pinset CLI and show an **Install CLI** action in the status bar.
- Offer a confirmed terminal-based install flow for the current local or remote extension host.
- Keep runtime, cache, receipt, and environment health findings in Pinset diagnostics instead of attaching them to line 1 of `pinset.toml`.

## 1.1.0

- Show the Pinset CLI version, resolved toolchain versions, and selected project environment directly in the VS Code status bar.
- Show a one-click `Pinset: Init` status action when the open folder has not been initialized.
- Activate after VS Code startup so uninitialized folders receive a visible entry point.

## 1.0.0

- Initial release with diagnostics, environment selection, project tasks, multi-root workspace support, and cancellable task terminals.
