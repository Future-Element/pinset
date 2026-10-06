# Pinset 3 for VS Code

The extension uses the same `pinset/3` reports as the CLI. It starts no process until VS Code Workspace Trust is granted. Set `pinset.executable` when the CLI is outside PATH.

Its five commands refresh status, install strictly from the project lock, check configuration and bindings, run explicit probes, and bind selected SDKs. Python points to the project's owned `.venv`; Flutter uses `.pinset/local/flutter-sdk`; Rust points to the locked compiler; Java updates the matching entry in `java.configuration.runtimes` while retaining other explicit entries.

The project Java runtime and Java language-server runtime have different requirements. Binding a Java 8 project never sets `java.jdt.ls.java.home` or substitutes that JDK for the language server. IDE binding is configuration evidence; an IDE process is not marked observed without a probe.

Binding edits the folder's `.vscode/settings.json` with a JSONC parser, preserving comments and unrelated values even before language extensions register their settings. Invalid JSON or unsaved settings block the edit.

There are no task providers, environment panels, workspace orchestration, old commands or CLI installation wizard. Install the paired CLI/shim through the official installer before using the extension. The extension's install command allows 30 minutes for upstream SDK downloads.

All checks and the real VS Code extension-host acceptance run through the repository's local Docker integration suite. `npm run package` compiles and packages only.
