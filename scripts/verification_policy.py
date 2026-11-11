"""Scope explicitly authorized for the local Pinset 3 verification suites."""

RUNTIME_EXEMPTIONS = [
    {
        'id': 'flutter-android-large-artifacts',
        'scope': 'Flutter SDK execution and Android SDK/APK build',
        'reason': 'User requested no download tests for large SDK artifacts such as Flutter',
        'status': 'exempt-unverified',
        'required_instead': 'Official metadata, exact locks, routing and deterministic Android evidence contracts',
    },
]

LARGE_ARTIFACT_BOUNDARY = 'Flutter SDK execution and Android APK build unverified; large-artifact download tests exempt by user request'

# This is the explicitly requested reduced scope for this class of hotfix,
# not all-suite acceptance. Publication still requires the user's authorization.
HOTFIX_SCOPE = 'Resolve symlinked CLI entries to their real executable for self-update, recovery and managed shim repair'
HOTFIX_COMMANDS = [
    ('format', ['cargo','fmt','--all','--','--check']),
    ('lint', ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings']),
    ('binaries', ['cargo','build','--workspace','--bins','--locked']),
    ('rust-contracts', ['cargo','test','--workspace','--locked']),
    ('application-contracts', ['python3','scripts/tests/contracts.py']),
    ('distribution-contracts', ['python3','scripts/tests/surfaces.py']),
]
HOTFIX_EVIDENCE = ['contracts.json','distribution.json'] + [label+'.log' for label,_ in HOTFIX_COMMANDS]
HOTFIX_UNVERIFIED = [
    'Official SDK download and runtime acceptance not rerun',
    'Linux ARM64/QEMU runtime and Windows/macOS target compilation not rerun',
    'Windows x86_64 native runtime not rerun',
    'macOS aarch64 native runtime not rerun',
    'Editor, website and credential-store integration acceptance not rerun',
    'Dependency security scans not rerun for this focused hotfix',
]

# Explicitly authorized Windows x86_64 host / Linux x86_64 Docker coverage.
# This is neither all-suite coverage nor the narrower historical hotfix scope.
X64_SCOPE = 'windows-x86_64-host-and-linux-x86_64-docker'
X64_ARM64_EXECUTION = 'Not run at user request; current host is Windows x86_64'
X64_SUITES = ['fast','acceptance','windows','integrations']
X64_COMMANDS = [
    ('format', ['cargo','fmt','--all','--','--check']),
    ('lint', ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings']),
    ('binaries', ['cargo','build','--workspace','--bins','--locked']),
    ('rust-contracts', ['cargo','test','--workspace','--locked']),
    ('cargo-security', ['python3','scripts/security_audit.py']),
    ('application-contracts', ['python3','scripts/tests/contracts.py']),
    ('standalone-migration-contracts', ['python3','-I','scripts/tests/migration.py']),
    ('distribution-contracts', ['python3','scripts/tests/surfaces.py']),
    ('acceptance-binaries', ['cargo','build','--workspace','--bins','--locked']),
    ('real-sdk-acceptance', ['python3','scripts/tests/acceptance.py']),
    ('windows-target', ['python3','scripts/tests/windows_checks.py']),
    ('integration-binaries', ['cargo','build','--workspace','--bins','--locked']),
    ('extension-dependencies', ['npm','ci','--ignore-scripts','--prefix','editors/vscode']),
    ('extension-types', ['npm','run','typecheck','--prefix','editors/vscode']),
    ('extension-security', ['npm','audit','--audit-level=high','--prefix','editors/vscode']),
    ('extension-unit', ['npm','run','test','--prefix','editors/vscode']),
    ('extension-package', ['npm','run','package','--prefix','editors/vscode']),
    ('website-dependencies', ['pnpm','--dir','website','install','--frozen-lockfile']),
    ('website-release-contracts', ['pnpm','--dir','website','test:release']),
    ('website-types', ['pnpm','--dir','website','typecheck']),
    ('website-security', ['pnpm','--dir','website','audit','--audit-level','high']),
    ('website-package', ['pnpm','--dir','website','build']),
    ('website-seo', ['pnpm','--dir','website','validate:seo']),
    ('runtime-integrations', ['dbus-run-session','--','bash','scripts/tests/integrations.sh']),
]
X64_EVIDENCE = [
    'contracts.json','distribution.json','cargo-security.json',
    'real-sdks.json','frozen-selectors.json','python-regressions.json',
    'toolchain-regressions.json','venv-recovery.json','windows-target.json',
    'integrations.json','editor-host.json','flutter-contracts.json','environment-access.json',
]
X64_UNVERIFIED = [
    'Windows x86_64 native runtime not run; only Windows x86_64 target compilation and path/credential contracts',
    'macOS target compilation and native runtime not run at user request',
    'ARM64 target compilation, QEMU emulation and native runtime not run at user request',
]
