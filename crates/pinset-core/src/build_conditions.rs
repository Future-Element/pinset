//! Detect external SDK footprints for explicitly requested build targets; never install or execute.
use crate::{EnvironmentCheck, ReadinessState};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn check(id: &str, state: ReadinessState, reason: &str, source: &str) -> EnvironmentCheck {
    EnvironmentCheck {
        id: format!("build.{id}.r1"),
        state,
        reason: format!("{reason}; rule revision 2026-09-12; application build not verified"),
        next_step: Some(source.to_owned()),
    }
}

fn footprint(path: &Path, file: bool) -> ReadinessState {
    match fs::metadata(path) {
        Ok(metadata)
            if if file {
                metadata.is_file()
            } else {
                metadata.is_dir()
            } =>
        {
            ReadinessState::Pass
        }
        Ok(_) => ReadinessState::Fail,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ReadinessState::Fail,
        Err(_) => ReadinessState::Unknown,
    }
}

fn contains_child(root: &Path, relative: &str) -> ReadinessState {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return ReadinessState::Fail,
        Err(_) => return ReadinessState::Unknown,
    };
    let mut unknown = false;
    for (index, entry) in entries.enumerate() {
        if index >= 128 {
            return ReadinessState::Unknown;
        }
        match entry {
            Ok(entry) => match footprint(&entry.path().join(relative), true) {
                ReadinessState::Pass => return ReadinessState::Pass,
                ReadinessState::Unknown => unknown = true,
                _ => {}
            },
            Err(_) => unknown = true,
        }
    }
    if unknown {
        ReadinessState::Unknown
    } else {
        ReadinessState::Fail
    }
}

/// Host and environment are explicit inputs for deterministic platform fixtures.
pub fn check_build_conditions(
    targets: &[String],
    host: &str,
    environment: &BTreeMap<String, String>,
) -> Vec<EnvironmentCheck> {
    let mut checks = Vec::new();
    let path = |name: &str| {
        environment
            .get(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    for target in targets {
        match target.as_str() {
            "android" => {
                let source = "https://docs.flutter.dev/platform-integration/android/setup";
                let home = path("ANDROID_HOME");
                let sdk_root = path("ANDROID_SDK_ROOT");
                if home
                    .as_ref()
                    .zip(sdk_root.as_ref())
                    .is_some_and(|(home, root)| {
                        fs::canonicalize(home)
                            .ok()
                            .zip(fs::canonicalize(root).ok())
                            .is_none_or(|(home, root)| home != root)
                    })
                {
                    checks.push(check(
                        "android.sdk",
                        ReadinessState::Unknown,
                        "Android SDK environment paths conflict",
                        source,
                    ));
                    continue;
                }
                let sdk = sdk_root.or(home).or_else(|| {
                    if host.starts_with("windows-") {
                        path("LOCALAPPDATA").map(|path| path.join("Android/Sdk"))
                    } else {
                        path("HOME").map(|path| {
                            path.join(if host.starts_with("macos-") {
                                "Library/Android/sdk"
                            } else {
                                "Android/Sdk"
                            })
                        })
                    }
                });
                let Some(sdk) = sdk else {
                    checks.push(check(
                        "android.sdk",
                        ReadinessState::Fail,
                        "Android SDK location missing",
                        source,
                    ));
                    continue;
                };
                let adb = if host.starts_with("windows-") {
                    "platform-tools/adb.exe"
                } else {
                    "platform-tools/adb"
                };
                checks.push(check(
                    "android.platform-tools",
                    footprint(&sdk.join(adb), true),
                    "Android platform tools footprint",
                    source,
                ));
                checks.push(check("android.platforms", contains_child(&sdk.join("platforms"), "android.jar"), "Android platform archive footprint; compileSdk compatibility requires an explicit project build", source));
                checks.push(check(
                    "android.licenses",
                    footprint(&sdk.join("licenses/android-sdk-license"), true),
                    "Android license record footprint; this does not accept a license",
                    source,
                ));
            }
            "windows" if !host.starts_with("windows-") => checks.push(check(
                "windows.host",
                ReadinessState::NotApplicable,
                "Windows build conditions apply on a Windows host",
                "https://docs.flutter.dev/platform-integration/windows/setup",
            )),
            "windows" => {
                let source = "https://docs.flutter.dev/platform-integration/windows/setup";
                let program_files = path("ProgramFiles(x86)");
                let mut visual_studios = path("VSINSTALLDIR").into_iter().collect::<Vec<_>>();
                if let Some(program_files) = &program_files {
                    for year in ["2022", "2026"] {
                        for edition in ["BuildTools", "Community", "Professional", "Enterprise"] {
                            visual_studios.push(
                                program_files
                                    .join("Microsoft Visual Studio")
                                    .join(year)
                                    .join(edition),
                            );
                        }
                    }
                }
                let installed = visual_studios.iter().any(|root| {
                    contains_child(&root.join("VC/Tools/MSVC"), "bin/Hostx64/x64/cl.exe")
                        == ReadinessState::Pass
                });
                checks.push(check("windows.cpp", if installed { ReadinessState::Pass } else { ReadinessState::Unknown }, "Visual Studio C++ compiler footprint; custom installation paths may require VSINSTALLDIR", source));
                let sdk = path("WindowsSdkDir")
                    .or_else(|| program_files.map(|path| path.join("Windows Kits/10")));
                checks.push(check(
                    "windows.sdk",
                    sdk.as_ref().map_or(ReadinessState::Unknown, |sdk| {
                        contains_child(&sdk.join("Include"), "um/Windows.h")
                    }),
                    "Windows SDK headers footprint",
                    source,
                ));
            }
            "ios" | "macos" if !host.starts_with("macos-") => checks.push(check(
                &format!("{target}.host"),
                ReadinessState::NotApplicable,
                "Apple build conditions apply on a macOS host",
                "https://docs.flutter.dev/platform-integration/ios/setup",
            )),
            "ios" | "macos" => {
                let source = "https://docs.flutter.dev/platform-integration/ios/setup";
                let developer = path("DEVELOPER_DIR")
                    .unwrap_or_else(|| PathBuf::from("/Applications/Xcode.app/Contents/Developer"));
                checks.push(check(&format!("{target}.xcode"), footprint(&developer.join("usr/bin/xcodebuild"), true), "Xcode executable footprint; configured custom installations should set DEVELOPER_DIR", source));
                let sdk = if target == "ios" {
                    "Platforms/iPhoneOS.platform/Developer/SDKs"
                } else {
                    "Platforms/MacOSX.platform/Developer/SDKs"
                };
                checks.push(check(
                    &format!("{target}.sdk"),
                    footprint(&developer.join(sdk), false),
                    "Apple platform SDK directory footprint",
                    source,
                ));
                checks.push(check(&format!("{target}.first-launch"), ReadinessState::Unknown, "Xcode license/first-launch completion requires an explicit native diagnostic task", source));
            }
            "linux" if !host.starts_with("linux-") => checks.push(check(
                "linux.host",
                ReadinessState::NotApplicable,
                "Linux desktop build conditions apply on Linux",
                "https://docs.flutter.dev/platform-integration/linux/setup",
            )),
            "linux" => {
                let source = "https://docs.flutter.dev/platform-integration/linux/setup";
                for command in ["clang++", "cmake", "ninja", "pkg-config"] {
                    let found = environment.get("PATH").is_some_and(|value| {
                        std::env::split_paths(value).take(128).any(|directory| {
                            directory.is_absolute()
                                && footprint(&directory.join(command), true) == ReadinessState::Pass
                        })
                    });
                    checks.push(check(
                        &format!("linux.{}", command.replace("++", "pp")),
                        if found {
                            ReadinessState::Pass
                        } else {
                            ReadinessState::Fail
                        },
                        "Linux build command footprint",
                        source,
                    ));
                }
                checks.push(check(
                    "linux.gtk",
                    ReadinessState::Unknown,
                    "GTK development package/version requires an explicit pkg-config or build task",
                    source,
                ));
            }
            _ => checks.push(check(
                "target",
                ReadinessState::Unknown,
                "Unrecognized build target",
                "Review requirements.build-targets",
            )),
        }
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_conditions_distinguish_missing_present_unknown_and_other_hosts() {
        let root = tempfile::tempdir().unwrap();
        let env = BTreeMap::from([("ANDROID_HOME".to_owned(), root.path().display().to_string())]);
        let checks = check_build_conditions(&["android".to_owned()], "linux-x86_64", &env);
        assert!(
            checks
                .iter()
                .all(|check| check.state == ReadinessState::Fail)
        );
        fs::create_dir_all(root.path().join("platform-tools")).unwrap();
        fs::write(root.path().join("platform-tools/adb"), "fixture").unwrap();
        assert_eq!(
            check_build_conditions(&["android".to_owned()], "linux-x86_64", &env)[0].state,
            ReadinessState::Pass
        );
        assert_eq!(
            check_build_conditions(&["windows".to_owned()], "linux-x86_64", &env)[0].state,
            ReadinessState::NotApplicable
        );
        assert_eq!(
            check_build_conditions(&["ios".to_owned()], "macos-aarch64", &env)
                .last()
                .unwrap()
                .state,
            ReadinessState::Unknown
        );
    }
}
