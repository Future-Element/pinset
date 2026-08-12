pub fn current_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub fn current_target_for_tool(tool: &str) -> String {
    let target = current_target();
    if tool != "bun" || std::env::consts::ARCH != "x86_64" {
        return target;
    }
    if x86_64_supports_avx2() {
        format!("{target}-avx2")
    } else {
        format!("{target}-baseline")
    }
}

#[cfg(target_arch = "x86_64")]
fn x86_64_supports_avx2() -> bool {
    std::is_x86_feature_detected!("avx2")
}

#[cfg(not(target_arch = "x86_64"))]
fn x86_64_supports_avx2() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_contains_os_and_architecture() {
        let target = current_target();
        assert!(target.contains(std::env::consts::OS));
        assert!(
            target.contains(std::env::consts::ARCH)
                || (std::env::consts::ARCH == "aarch64" && target.contains("aarch64"))
        );
    }

    #[test]
    fn bun_x64_target_records_cpu_variant() {
        let target = current_target_for_tool("bun");
        if std::env::consts::ARCH == "x86_64" {
            assert!(target.ends_with("-avx2") || target.ends_with("-baseline"));
        }
    }
}
