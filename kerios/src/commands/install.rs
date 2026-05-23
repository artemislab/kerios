use anyhow::{anyhow, Result};

const MACOS_PLIST: &str = include_str!("../../../packaging/launchd/io.artemislab.kerios.plist");
const LINUX_SERVICE: &str = include_str!("../../../packaging/systemd/kerios.service");

/// Render the service definition for the given OS, with the absolute
/// `binary` path substituted into the template's `{BINARY}` placeholder.
///
/// # Errors
/// Returns an error if the OS is not supported.
pub fn generate(os: &str, binary: &str) -> Result<String> {
    match os {
        "macos" => Ok(MACOS_PLIST.replace("{BINARY}", binary)),
        "linux" => Ok(LINUX_SERVICE.replace("{BINARY}", binary)),
        other => Err(anyhow!(
            "unsupported OS for `kerios install`: {other} (supported: macos, linux)"
        )),
    }
}

/// Entry point for `kerios install`.
pub fn run() -> Result<()> {
    let binary = std::env::current_exe()?;
    let unit = generate(std::env::consts::OS, &binary.display().to_string())?;
    println!("{unit}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_macos_plist_contains_label_and_binary() {
        let out = generate("macos", "/usr/local/bin/kerios").unwrap();
        assert!(out.contains("<key>Label</key>"));
        assert!(out.contains("io.artemislab.kerios"));
        assert!(out.contains("/usr/local/bin/kerios"));
        assert!(!out.contains("{BINARY}"), "placeholder must be substituted");
    }

    #[test]
    fn generate_linux_systemd_unit_contains_execstart_and_binary() {
        let out = generate("linux", "/home/alice/.local/bin/kerios").unwrap();
        assert!(out.contains("[Service]"));
        assert!(out.contains("ExecStart=/home/alice/.local/bin/kerios daemon"));
        assert!(!out.contains("{BINARY}"));
    }

    #[test]
    fn generate_unsupported_os_is_an_error() {
        let result = generate("plan9", "/bin/kerios");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("plan9"), "error should name the OS: {msg}");
    }
}
