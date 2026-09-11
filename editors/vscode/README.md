# Pinset for VS Code

The extension reads the versioned, secret-free `pinset editor context --json` protocol. The VS Code status bar shows the Pinset CLI version, resolved toolchain versions, and selected project environment. It also publishes diagnostics, switches environment profiles, and runs declared tasks in single-folder or multi-root workspaces.

Configuration findings are attached to `pinset.toml`, and lock or provenance findings are attached to `pinset.lock`. Runtime installation, cache, receipt, and Python environment health remain available through **Pinset: Check Diagnostics** without appearing as misleading line-1 file errors.

When an open folder does not contain a Pinset project, the status bar shows **Pinset: Init**. Select it to run `pinset init`, create `pinset.toml`, and refresh the project status immediately.

When the Pinset CLI is not installed in the current extension host, the status bar shows **Pinset: Install CLI**. Select it to review and start the official checksum-verifying installer in a dedicated terminal, or open the installation guide. The extension records the installed executable path so **Pinset: Refresh Status** works after installation completes.

Workspace Trust is required before the extension starts Pinset or a project task. Cancelling a Pinset task terminates its process group on macOS/Linux and its process tree on Windows.

Configure `pinset.executablePath` when `pinset` is not available on the extension host's `PATH`.

## Install

Install Pinset from the Visual Studio Marketplace:

```sh
code --install-extension FutureElement.pinset-vscode
```

The extension and Pinset CLI must be installed in the same local or remote extension host. Each folder in a multi-root workspace is discovered and refreshed independently.

## Commands

- **Pinset: Refresh Status** refreshes every open workspace folder.
- **Pinset: Install CLI** installs Pinset in the current local or remote extension host after confirmation.
- **Pinset: Initialize Project** creates a minimal `pinset.toml` in the active folder.
- **Pinset: Select Environment** saves or resets the machine-local profile for the active folder.
- **Pinset: Run Project Task** runs a declared task in a cancellable VS Code task terminal.
- **Pinset: Check Diagnostics** publishes current findings and opens the Pinset output channel.

The status bar uses separate, clickable items for the Pinset version and diagnostics, toolchain versions, and selected environment. Task command arrays and environment values are never included in the editor protocol.
