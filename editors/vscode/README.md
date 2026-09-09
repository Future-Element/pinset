# Pinset for VS Code

The extension reads the versioned, secret-free `pinset editor context --json` protocol. It shows the active toolchain state, publishes diagnostics, switches project environment profiles, and runs declared tasks in single-folder or multi-root workspaces.

Workspace Trust is required before the extension starts Pinset or a project task. Cancelling a Pinset task terminates its process group on macOS/Linux and its process tree on Windows.

Configure `pinset.executablePath` when `pinset` is not available on the extension host's `PATH`.

## Install

Download `pinset-vscode-1.0.0.vsix` from the Pinset 2.12.1 GitHub Release, then run:

```sh
code --install-extension pinset-vscode-1.0.0.vsix
```

The extension and Pinset CLI must be installed in the same local or remote extension host. Each folder in a multi-root workspace is discovered and refreshed independently.

## Commands

- **Pinset: Refresh Status** refreshes every open workspace folder.
- **Pinset: Select Environment** saves or resets the machine-local profile for the active folder.
- **Pinset: Run Project Task** runs a declared task in a cancellable VS Code task terminal.
- **Pinset: Check Diagnostics** publishes current findings and opens the Pinset output channel.

The status-bar item shows the active folder's selected profile and diagnostic state. Task command arrays and environment values are never included in the editor protocol.
