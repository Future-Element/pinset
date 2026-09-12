# Change Log

## 1.2.0 (unreleased)

- Add per-folder environment preparation, readiness and execution evidence.
- Preview and restore Node, Python and Flutter workspace bindings.
- Observe controlled native debugger launches independently of configuration readiness.
- Negotiate protocol 2 with CLI 2.13+ while retaining protocol 1 compatibility.

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
