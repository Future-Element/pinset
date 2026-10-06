//! Human-readable reports are presentation only; JSON remains the shared contract.
use serde_json::Value;
use std::fmt::Write;
fn string(value: &Value) -> &str {
    value.as_str().unwrap_or("-")
}
fn yes(value: &Value, zh: bool) -> &str {
    if value == true {
        if zh { "是" } else { "yes" }
    } else if zh {
        "否"
    } else {
        "no"
    }
}
fn tools(output: &mut String, value: &Value) {
    if let Some(items) = value["tool"].as_array() {
        for tool in items {
            let _ = writeln!(
                output,
                "  {}  {}  ({})",
                string(&tool["name"]),
                string(&tool["version"]),
                string(&tool["requested"])
            );
        }
    }
}
fn route(output: &mut String, value: &Value, zh: bool) {
    let _ = writeln!(
        output,
        "{}  {}",
        string(&value["command"]),
        string(&value["version"])
    );
    for (key, label) in [
        ("executable", if zh { "入口" } else { "Entry" }),
        ("sdk", "SDK"),
        ("provider", if zh { "发行来源" } else { "Provider" }),
        ("build", if zh { "构建号" } else { "Build" }),
        ("source", if zh { "选择依据" } else { "Selection" }),
    ] {
        if let Some(text) = value[key].as_str() {
            let _ = writeln!(output, "  {label}: {text}");
        }
    }
}
pub fn report(value: &Value, zh: bool) -> String {
    let mut output = String::new();
    if value["plan"] == true {
        let _ = writeln!(
            output,
            "{}",
            if zh {
                "预览：不会写入任何状态"
            } else {
                "Preview: no state changes"
            }
        );
    }
    if let Some(checks) = value["report"]["checks"].as_array() {
        let _ = writeln!(
            output,
            "{}",
            if zh {
                "工具  配置  安装  绑定  实际探针"
            } else {
                "Tool  Configured  Installed  Bound  Observed"
            }
        );
        for check in checks {
            let _ = writeln!(
                output,
                "{}  {}  {}  {}  {}",
                string(&check["tool"]),
                yes(&check["configured"], zh),
                yes(&check["installed"], zh),
                yes(&check["bound"], zh),
                yes(&check["actually_verified"], zh)
            );
            if check["bound"] != true || check["installed"] != true {
                let _ = writeln!(output, "  {}", string(&check["detail"]));
            }
        }
        if !value["java"].is_null() {
            let java = &value["java"];
            let _ = writeln!(
                output,
                "JDK: {} ({})",
                string(&java["project_jdk"]),
                string(&java["version"])
            );
            for name in ["maven", "gradle"] {
                let observed = &java["build_runtime_jdk"]["observations"][name];
                if observed["actual_observed"] == true {
                    let _ = writeln!(
                        output,
                        "  {name}: {}  {}: {}",
                        string(&observed["runtime_home"]),
                        if zh {
                            "匹配项目 JDK"
                        } else {
                            "matches project JDK"
                        },
                        yes(&observed["runtime_matches_lock"], zh)
                    );
                }
            }
            let _ = writeln!(
                output,
                "  {}",
                if zh {
                    "编译 JDK、字节码目标和 IDE 进程各自需要观测；入口探针不代表构建验证。"
                } else {
                    "Compiler JDK, bytecode targets and IDE processes require separate observations; entry probes do not verify a build."
                }
            );
        }
        if !value["android"].is_null() {
            let _ = writeln!(
                output,
                "Android {}: {}",
                if zh {
                    "环境观测"
                } else {
                    "environment observed"
                },
                yes(&value["android"]["verified"], zh)
            );
        }
        return output;
    }
    if value["command"].is_string() {
        route(&mut output, value, zh);
        return output;
    }
    if let Some(commands) = value["commands"].as_array() {
        for command in commands {
            route(&mut output, command, zh);
        }
        return output;
    }
    if let Some(releases) = value["releases"].as_array() {
        for release in releases {
            let _ = writeln!(
                output,
                "{}",
                release
                    .as_str()
                    .unwrap_or_else(|| string(&release["version"]))
            );
        }
        return output;
    }
    if value["selected"]["tool"].is_array() {
        let _ = writeln!(
            output,
            "{}",
            if zh {
                "所选工具（精确锁）"
            } else {
                "Selected tools (exact lock)"
            }
        );
        tools(&mut output, &value["selected"]);
        if let Some(installs) = value["installs"].as_array() {
            let _ = writeln!(output, "{}", if zh { "已安装" } else { "Installed" });
            for item in installs {
                let _ = writeln!(
                    output,
                    "  {}  {}  {}  {}",
                    string(&item["tool"]),
                    string(&item["version"]),
                    string(&item["platform"]),
                    string(&item["path"])
                );
            }
        }
        return output;
    }
    if let Some(selected) = value["selected"].as_object() {
        if selected.is_empty() {
            let _ = writeln!(
                output,
                "{}",
                if zh {
                    "未选择工具"
                } else {
                    "No selected tools"
                }
            );
        }
        for (name, selector) in selected {
            let _ = writeln!(output, "{name}@{}", string(selector));
        }
        if let Some(notes) = value["notes"].as_array() {
            for note in notes {
                let _ = writeln!(output, "{}", string(note));
            }
        }
        return output;
    }
    if value["lock"]["tool"].is_array() {
        tools(&mut output, &value["lock"]);
        return output;
    }
    let candidate = if value["candidate"]["id"].is_string() {
        &value["candidate"]
    } else {
        value
    };
    if candidate["candidate"]["tool"].is_array() {
        let _ = writeln!(
            output,
            "{}: {}",
            if zh { "候选" } else { "Candidate" },
            string(&candidate["id"])
        );
        tools(&mut output, &candidate["candidate"]);
        if let Some(test) = candidate["last_test"].as_object() {
            let _ = writeln!(
                output,
                "{}: baseline={}, candidate={}, limited={}",
                if zh {
                    "最近验证退出码"
                } else {
                    "Latest validation exits"
                },
                test.get("baseline_exit").unwrap_or(&Value::Null),
                test.get("candidate_exit").unwrap_or(&Value::Null),
                test.get("limited")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
            );
        }
        return output;
    }
    // Small profile/maintenance reports keep their public names and paths, never values.
    if let Some(object) = value.as_object() {
        for (key, item) in object {
            if [
                "protocol",
                "transaction",
                "fingerprint",
                "source_fingerprint",
                "state_fingerprint",
            ]
            .contains(&key.as_str())
            {
                continue;
            }
            match item {
                Value::Null => {}
                Value::String(text) => {
                    let _ = writeln!(output, "{key}: {text}");
                }
                Value::Bool(_) => {
                    let _ = writeln!(output, "{key}: {}", yes(item, zh));
                }
                Value::Array(items) => {
                    let _ = writeln!(output, "{key}: {}", items.len());
                    for entry in items {
                        if let Some(text) = entry.as_str() {
                            let _ = writeln!(output, "  {text}");
                        } else if entry["id"].is_string() {
                            let _ = writeln!(output, "  {}", string(&entry["id"]));
                        }
                    }
                }
                _ => {
                    let _ = writeln!(output, "{key}: {item}");
                }
            }
        }
    }
    if output.is_empty() {
        let _ = writeln!(output, "{}", if zh { "已完成" } else { "Done" });
    }
    output
}
