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
    if value["operation"] == "import" {
        return import_report(value, zh);
    }
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
            if let Some(status) = check["source_verification"]["status"].as_str() {
                let label = match (status, zh) {
                    ("signature-verified", true) => "已验签名",
                    ("signature-verified", false) => "signature verified",
                    ("digest-only", true) => "仅摘要固定",
                    ("digest-only", false) => "digest pinned only",
                    (_, true) => "来源验证失败",
                    (_, false) => "source verification failed",
                };
                let _ = writeln!(output, "  {label}");
            }
            if check["passed"] == false || check["bound"] != true || check["installed"] != true {
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
        let _ = writeln!(
            output,
            "{}: {}",
            if zh { "检查通过" } else { "Check passed" },
            yes(&value["passed"], zh)
        );
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
    // Small profile/maintenance reports keep their public names and paths, never values.
    if let Some(object) = value.as_object() {
        for (key, item) in object {
            if ["protocol", "transaction", "fingerprint"].contains(&key.as_str()) {
                continue;
            }
            if value["installed"] == true && ["tools", "installed_tools"].contains(&key.as_str()) {
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

fn import_report(value: &Value, zh: bool) -> String {
    if value["cancelled"] == true {
        return if zh {
            "已取消导入，未写入文件。\n"
        } else {
            "Import cancelled; no files written.\n"
        }
        .into();
    }
    let mut output = String::new();
    let _ = writeln!(
        output,
        "{}",
        match (zh, value["plan"] == true) {
            (true, true) => "迁移计划（尚未写入）",
            (false, true) => "Import plan (no files written)",
            (true, false) => "导入结果",
            (false, false) => "Import result",
        }
    );
    let _ = writeln!(
        output,
        "{}: {}",
        if zh { "项目" } else { "Project" },
        string(&value["project"])
    );
    for (key, en, cn) in [
        ("imports", "Import", "导入"),
        ("retained", "Keep existing", "保留已有选择"),
        (
            "unsupported",
            "No matching Provider (skipped)",
            "无对应 Provider（跳过）",
        ),
        ("conflicts", "Conflicts (skipped)", "冲突（跳过）"),
        ("unresolved", "Cannot resolve (skipped)", "无法解析（跳过）"),
    ] {
        let Some(items) = value[key].as_array().filter(|items| !items.is_empty()) else {
            continue;
        };
        let _ = writeln!(output, "{}:", if zh { cn } else { en });
        for item in items {
            let _ = write!(output, "  {}", string(&item["tool"]));
            if let Some(version) = item["version"]
                .as_str()
                .or_else(|| item["requested"].as_str())
            {
                let _ = write!(output, "@{version}");
            }
            if item["resolved"] == true {
                let _ = write!(
                    output,
                    " ({})",
                    if zh {
                        "已解析为确切版本"
                    } else {
                        "resolved to an exact version"
                    }
                );
            }
            if let Some(selector) = item["selector"].as_str().filter(|_| key == "retained") {
                let _ = write!(output, " ({selector})");
            }
            let _ = writeln!(output);
            if let Some(declarations) = item["declarations"].as_array() {
                for declaration in declarations {
                    let _ = writeln!(
                        output,
                        "    {}: {}",
                        string(&declaration["source"]),
                        string(&declaration["requested"])
                    );
                }
            } else if let Some(source) = item["source"].as_str() {
                let _ = writeln!(output, "    {source}");
            }
            if let Some(reason) = item["reason"].as_str() {
                let reason = match (zh, reason) {
                    (true, "existing_selection") => "与已有选择冲突，保留已有选择",
                    (false, "existing_selection") => {
                        "conflicts with the existing selection; keep existing"
                    }
                    (true, "incompatible_sources") => "多个来源的版本声明不兼容",
                    (false, "incompatible_sources") => {
                        "source version declarations are incompatible"
                    }
                    (true, "existing_rust_options") => "与已有 Rust 选项冲突",
                    (false, "existing_rust_options") => "conflicts with existing Rust options",
                    (_, other) => other,
                };
                let _ = writeln!(output, "    {reason}");
            }
        }
    }
    if value["requires_write"] == false {
        let _ = writeln!(
            output,
            "{}",
            if zh {
                "没有需要写入的更改。"
            } else {
                "No changes to write."
            }
        );
    } else {
        let _ = writeln!(
            output,
            "{}: {}\n{}: {}",
            if zh { "配置" } else { "Config" },
            string(&value["config"]),
            if zh { "精确锁" } else { "Lock" },
            string(&value["lock"])
        );
        let _ = writeln!(
            output,
            "{}",
            if value["applied"] == true {
                if zh {
                    "配置和锁文件已写入。使用 pinset install 安装工具。"
                } else {
                    "Config and lock written. Run pinset install to install tools."
                }
            } else if zh {
                "确认后只写入配置和锁文件，不安装工具。"
            } else {
                "Confirmation writes config and lock only; tools are not installed."
            }
        );
    }
    output
}
