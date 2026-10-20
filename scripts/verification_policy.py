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
