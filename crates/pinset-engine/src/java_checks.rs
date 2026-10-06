//! Build declarations are separate from observations. Probing a wrapper can
//! execute project build logic; it never proves a compilation took place.
use crate::execution::prepare_command;
use crate::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

const GRADLE_PROBE: &str = r#"
gradle.projectsEvaluated {
  gradle.rootProject.tasks.register('pinsetEnvironmentProbe') {
    doLast {
      println('PINSET_JAVA_RUNTIME=' + System.getProperty('java.home'))
      println('PINSET_GRADLE_VERSION=' + gradle.gradleVersion)
      gradle.rootProject.allprojects.each { p ->
        p.tasks.withType(org.gradle.api.tasks.compile.JavaCompile).each { t ->
          def home = System.getProperty('java.home')
          if (t.hasProperty('javaCompiler') && t.javaCompiler.present) {
            home = t.javaCompiler.get().metadata.installationPath.asFile.absolutePath
          }
          def release = t.options.hasProperty('release') && t.options.release.present ? t.options.release.get().toString() : ''
          println('PINSET_JAVA_COMPILER=' + groovy.json.JsonOutput.toJson([
            task:t.path, home:home, source:t.sourceCompatibility,
            target:t.targetCompatibility, release:release,
            fork:t.options.fork, forkExecutable:t.options.forkOptions.executable
          ]))
        }
        def plugin = p.plugins.findPlugin('com.android.application') ?: p.plugins.findPlugin('com.android.library')
        if (plugin != null) {
          try {
            def components = p.extensions.findByName('androidComponents')
            def sdk = components != null ? components.sdkComponents.sdkDirectory.get().asFile : p.extensions.findByName('android').sdkDirectory
            println('PINSET_ANDROID_SDK=' + sdk.canonicalPath)
          } catch (Exception ignored) { println('PINSET_ANDROID_SDK=unobserved') }
          try { println('PINSET_AGP_VERSION=' + plugin.class.classLoader.loadClass('com.android.Version').getField('ANDROID_GRADLE_PLUGIN_VERSION').get(null)) }
          catch (Exception ignored) { println('PINSET_AGP_VERSION=unobserved') }
        }
      }
    }
  }
}
"#;

fn wrapper(root: &Path, name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) {
        format!("{name}.bat")
    } else {
        name.to_string()
    };
    root.join(file).is_file().then(|| {
        root.join(if cfg!(windows) {
            format!("{name}.bat")
        } else {
            name.to_string()
        })
    })
}
fn same_jdk(observed: &str, expected: &Path) -> bool {
    let path = Path::new(observed);
    let path = if path.file_name().is_some_and(|n| n == "jre") {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    path.canonicalize()
        .ok()
        .zip(expected.canonicalize().ok())
        .is_some_and(|(a, b)| a == b)
}
fn flutter_android_observation(
    code: i32,
    output: &str,
    jdk: &Path,
    gradle: &Value,
) -> (Value, bool) {
    let binary = output.lines().find_map(|line| {
        line.find("Java binary at:")
            .map(|i| line[i + 15..].trim().to_owned())
    });
    let matches = binary
        .as_deref()
        .and_then(|path| Path::new(path).parent())
        .and_then(Path::parent)
        .is_some_and(|path| same_jdk(&path.to_string_lossy(), jdk));
    let flutter_sdk = output.lines().find_map(|line| {
        line.find("Android SDK at ")
            .map(|i| line[i + 15..].trim().to_owned())
    });
    let sdk_matches = flutter_sdk
        .as_deref()
        .zip(gradle["android_sdk"].as_str())
        .is_some_and(|(a, b)| {
            Path::new(a)
                .canonicalize()
                .ok()
                .zip(Path::new(b).canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
        });
    let observed = json!({"exit_code":code,"java_binary":binary,"actual_jdk_observed":binary.is_some(),"jdk_matches_lock":matches,"android_sdk":flutter_sdk,"sdk_matches_gradle":sdk_matches,"scope":"Flutter doctor selected Android JDK; IDE process not observed"});
    let verified = code == 0
        && matches
        && sdk_matches
        && gradle["runtime_matches_lock"] == true
        && gradle["agp_version"]
            .as_str()
            .is_some_and(|v| !v.is_empty() && v != "unobserved")
        && gradle["android_sdk"]
            .as_str()
            .is_some_and(|p| p != "unobserved" && Path::new(p).is_dir());
    (observed, verified)
}
fn declarations(root: &Path, files: &[&str]) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let (mut runtime, mut compiler, mut targets) = (vec![], vec![], vec![]);
    for file in files {
        let p = root.join(file);
        if !p.is_file() {
            continue;
        }
        for line in fs::read_to_string(p)?.lines() {
            let entry = json!({"file":file,"declaration":line.trim(),"actual_observed":false});
            if ["java.home", "javaHome", "java.installations", "jdkHome"]
                .iter()
                .any(|v| line.contains(v))
            {
                runtime.push(entry.clone());
            }
            if [
                "toolchain",
                "JavaLanguageVersion",
                "jdkToolchain",
                "<jdk>",
                "<executable>",
            ]
            .iter()
            .any(|v| line.contains(v))
            {
                compiler.push(entry.clone());
            }
            if [
                "sourceCompatibility",
                "targetCompatibility",
                "maven.compiler",
                "<source>",
                "<target>",
                "<release>",
                "--release",
            ]
            .iter()
            .any(|v| line.contains(v))
            {
                targets.push(entry);
            }
        }
    }
    Ok((runtime, compiler, targets))
}
impl Services {
    fn build_probe(
        &self,
        c: &ProjectContext,
        args: &[String],
        cwd: &Path,
    ) -> Result<(i32, String)> {
        let plan = plan_command(cwd, &self.home, c, &args[0])?;
        let mut command = prepare_command(&plan, &args[1..], cwd, BTreeMap::new());
        command.env("PINSET_NO_ENV", "1");
        let out = crate::process_tree::run(command, Duration::from_secs(120), Some(1024 * 1024))
            .map_err(|e| service_error("PINSET_PROBE_FAILED", e.to_string()))?;
        Ok((out.code, String::from_utf8_lossy(&out.stdout).into_owned()))
    }
    fn gradle_observation(&self, c: &ProjectContext, root: &Path, sdk: &Path) -> Result<Value> {
        let Some(wrapper) = wrapper(root, "gradlew") else {
            return Ok(json!({"present":false,"actual_observed":false}));
        };
        let path = self
            .home
            .join("state/probes")
            .join(format!("gradle-{}.gradle", uuid::Uuid::new_v4()));
        write_atomic(&path, GRADLE_PROBE.as_bytes())?;
        let args = vec![
            wrapper.to_string_lossy().into_owned(),
            "--offline".into(),
            "--no-daemon".into(),
            "--console=plain".into(),
            "-Dorg.gradle.java.installations.auto-download=false".into(),
            "-I".into(),
            path.to_string_lossy().into_owned(),
            "pinsetEnvironmentProbe".into(),
        ];
        let result = self.build_probe(c, &args, root);
        let _ = fs::remove_file(path);
        let (code, output) = result?;
        let home = output
            .lines()
            .find_map(|l| l.trim().strip_prefix("PINSET_JAVA_RUNTIME="))
            .map(str::to_owned);
        let compilers = output
            .lines()
            .filter_map(|l| l.trim().strip_prefix("PINSET_JAVA_COMPILER="))
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .map(|mut v| {
                let fork_external = v["fork"] == true
                    && v["forkExecutable"]
                        .as_str()
                        .filter(|p| !p.is_empty())
                        .is_some_and(|p| {
                            Path::new(p)
                                .parent()
                                .and_then(Path::parent)
                                .is_none_or(|root| !same_jdk(&root.to_string_lossy(), sdk))
                        });
                v["external"] =
                    json!(fork_external || !v["home"].as_str().is_some_and(|h| same_jdk(h, sdk)));
                v["configuration_observed"] = json!(true);
                v["compilation_observed"] = json!(false);
                v
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"present":true,"exit_code":code,"actual_observed":code==0&&home.is_some(),"runtime_home":home,"runtime_matches_lock":code==0&&home.as_deref().is_some_and(|h|same_jdk(h,sdk)),"gradle_version":output.lines().find_map(|l|l.trim().strip_prefix("PINSET_GRADLE_VERSION=")),"agp_version":output.lines().find_map(|l|l.trim().strip_prefix("PINSET_AGP_VERSION=")),"android_sdk":output.lines().find_map(|l|l.trim().strip_prefix("PINSET_ANDROID_SDK=")),"compiler_configurations":compilers,"scope":"wrapper configuration and actual build JVM; no compilation performed"}),
        )
    }
    pub(crate) fn java_build_report(
        &self,
        c: &ProjectContext,
        lock: &Lockfile,
        probe: bool,
    ) -> Result<Value> {
        let tool = lock.tool("java").unwrap();
        let platform = locked_target(tool);
        let sdk = sdk_root(
            &install_directory(&self.home, tool, &platform),
            "java",
            &platform,
        );
        let (runtime, compiler, targets) = declarations(
            &c.root,
            &[
                "pom.xml",
                "build.gradle",
                "build.gradle.kts",
                "gradle.properties",
                ".mvn/jvm.config",
                ".mvn/toolchains.xml",
            ],
        )?;
        let mut observations =
            json!({"gradle":{"actual_observed":false},"maven":{"actual_observed":false}});
        if probe {
            observations["gradle"] = self.gradle_observation(c, &c.root, &sdk)?;
            if let Some(wrapper) = wrapper(&c.root, "mvnw") {
                let (code, output) = self.build_probe(
                    c,
                    &[wrapper.to_string_lossy().into_owned(), "-version".into()],
                    &c.root,
                )?;
                let home = output.lines().find_map(|l| {
                    l.find("Java home:")
                        .map(|i| l[i + 10..].trim().to_owned())
                        .or_else(|| l.find("runtime:").map(|i| l[i + 8..].trim().to_owned()))
                });
                observations["maven"] = json!({"present":true,"exit_code":code,"actual_observed":code==0&&home.is_some(),"runtime_home":home,"runtime_matches_lock":code==0&&home.as_deref().is_some_and(|h|same_jdk(h,&sdk)),"compiler_toolchain_observed":false,"scope":"Maven runtime only; compiler plugins and toolchains require build evidence"});
            }
        }
        let overrides = [
            "JAVA_TOOL_OPTIONS",
            "JDK_JAVA_OPTIONS",
            "GRADLE_JAVA_HOME",
            "GRADLE_OPTS",
            "MAVEN_OPTS",
        ]
        .into_iter()
        .filter(|k| env::var_os(k).is_some())
        .collect::<Vec<_>>();
        Ok(
            json!({"project_jdk":sdk,"version":tool.version,"build":tool.metadata.get("build"),"provider":tool.provider,"build_runtime_jdk":{"binding":sdk,"external_declarations":runtime,"observations":observations},"compiler_toolchain_jdk":{"actual_observed":false,"external_declarations":compiler,"configuration_observations":observations["gradle"]["compiler_configurations"]},"bytecode_target":{"actual_observed":false,"declarations":targets},"process_override_names":overrides,"ide_project_jdk":{"binding":sdk,"actual_observed":false},"language_server_jdk":{"actual_observed":false,"binding":"configured separately by the Java language extension"}}),
        )
    }
    pub(crate) fn android_report(
        &self,
        c: &ProjectContext,
        lock: &Lockfile,
        probe: bool,
    ) -> Result<Value> {
        let (root, tool) = (c.root.join("android"), lock.tool("java"));
        let (runtime, _, targets) = declarations(
            &root,
            &[
                "gradle.properties",
                "local.properties",
                "build.gradle",
                "build.gradle.kts",
                "settings.gradle",
                "settings.gradle.kts",
            ],
        )?;
        let sdk = env::var_os("ANDROID_HOME")
            .or_else(|| env::var_os("ANDROID_SDK_ROOT"))
            .map(PathBuf::from)
            .or_else(|| {
                fs::read_to_string(root.join("local.properties"))
                    .ok()
                    .and_then(|text| {
                        text.lines().find_map(|line| {
                            line.trim().strip_prefix("sdk.dir=").map(|value| {
                                let path =
                                    PathBuf::from(value.replace("\\:", ":").replace("\\\\", "\\"));
                                if path.is_absolute() {
                                    path
                                } else {
                                    root.join(path)
                                }
                            })
                        })
                    })
            });
        let mut value = json!({"flutter_selected":lock.tool("flutter").is_some(),"java_selected":tool.is_some(),"android_sdk":sdk,"external_declarations":runtime,"target_declarations":targets,"flutter":{"actual_jdk_observed":false},"gradle":{"actual_observed":false},"android_studio_jdk_observed":false,"verified":false});
        if probe && let Some(tool) = tool {
            let platform = locked_target(tool);
            let jdk = sdk_root(
                &install_directory(&self.home, tool, &platform),
                "java",
                &platform,
            );
            value["gradle"] = self.gradle_observation(c, &root, &jdk)?;
            if lock.tool("flutter").is_some() {
                let (code, output) = self.build_probe(
                    c,
                    &["flutter".into(), "doctor".into(), "-v".into()],
                    &c.root,
                )?;
                let (flutter, verified) =
                    flutter_android_observation(code, &output, &jdk, &value["gradle"]);
                value["flutter"] = flutter;
                value["verified"] = json!(verified);
            }
        }
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn android_requires_matching_jdk_sdk_and_successful_observations() {
        // Output fixtures exercise evidence composition, never an actual SDK or APK build.
        let root = tempfile::tempdir().unwrap();
        let jdk = root.path().join("jdk");
        let sdk = root.path().join("android-sdk");
        let external = root.path().join("external");
        for path in [&jdk, &sdk, &external] {
            fs::create_dir_all(path).unwrap();
        }
        let output = format!(
            "Java binary at: {}/bin/java\nAndroid SDK at {}\n",
            jdk.display(),
            sdk.display()
        );
        let gradle = json!({"runtime_matches_lock":true,"agp_version":"8.7.0","android_sdk":sdk});
        assert!(flutter_android_observation(0, &output, &jdk, &gradle).1);
        assert!(!flutter_android_observation(1, &output, &jdk, &gradle).1);
        assert!(!flutter_android_observation(0, &output, &external, &gradle).1);
        let mut drift = gradle.clone();
        drift["android_sdk"] = json!(external);
        assert!(!flutter_android_observation(0, &output, &jdk, &drift).1);
        drift = gradle.clone();
        drift["runtime_matches_lock"] = json!(false);
        assert!(!flutter_android_observation(0, &output, &jdk, &drift).1);
        drift = gradle.clone();
        drift["agp_version"] = json!("");
        assert!(!flutter_android_observation(0, &output, &jdk, &drift).1);
        let (unobserved, verified) = flutter_android_observation(0, "", &jdk, &gradle);
        assert!(!verified && unobserved["actual_jdk_observed"] == false);
    }
    #[test]
    fn separates_target_and_external_toolchain_declarations() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("build.gradle"),"java { toolchain { languageVersion = JavaLanguageVersion.of(17) } }\nsourceCompatibility = 1.8\norg.gradle.java.home=/external/jdk").unwrap();
        let (r, c, t) = declarations(root.path(), &["build.gradle"]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(c.len(), 1);
        assert_eq!(t.len(), 1);
    }
}
