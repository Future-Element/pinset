pub fn current_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
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
}
