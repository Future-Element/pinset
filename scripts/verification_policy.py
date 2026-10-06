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
