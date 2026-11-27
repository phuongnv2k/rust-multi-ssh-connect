use crate::models::{AppSettings, AuthMethod, SftpAppKind, SshProfile, TerminalAppKind};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const ALACRITTY_URL: &str =
    "https://github.com/alacritty/alacritty/releases/download/v0.17.0/Alacritty-v0.17.0-portable.exe";
const WINSCP_URL: &str =
    "https://winscp.net/download/WinSCP-6.7.1-Portable.zip/download";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedTerminal {
    Alacritty(PathBuf),
    WindowsTerminal,
    PowerShell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedSftp {
    WinScp(PathBuf),
    FileZilla(PathBuf),
    OpenSshSftp,
}

pub fn tools_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.join("tools");
        }
    }
    PathBuf::from("tools")
}

pub fn bundled_alacritty_path() -> PathBuf {
    tools_dir().join("alacritty.exe")
}

pub fn bundled_winscp_path() -> PathBuf {
    tools_dir().join("WinSCP.exe")
}

pub fn has_bundled_tools() -> (bool, bool) {
    (
        bundled_alacritty_path().exists(),
        bundled_winscp_path().exists(),
    )
}

/// Pure filesystem check — NEVER spawns `where.exe` so zero console flashes!
pub fn detect_terminal() -> DetectedTerminal {
    let ala = bundled_alacritty_path();
    if ala.exists() {
        return DetectedTerminal::Alacritty(ala);
    }
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        let wt = PathBuf::from(local_app)
            .join("Microsoft")
            .join("WindowsApps")
            .join("wt.exe");
        if wt.exists() {
            return DetectedTerminal::WindowsTerminal;
        }
    }
    DetectedTerminal::PowerShell
}

/// Pure filesystem check — NEVER spawns any subprocess!
pub fn detect_sftp() -> DetectedSftp {
    let bundled = bundled_winscp_path();
    if bundled.exists() {
        return DetectedSftp::WinScp(bundled);
    }

    let winscp_candidates = [
        r"C:\Program Files (x86)\WinSCP\WinSCP.exe",
        r"C:\Program Files\WinSCP\WinSCP.exe",
    ];
    for p in winscp_candidates {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return DetectedSftp::WinScp(pb);
        }
    }

    let filezilla_candidates = [
        r"C:\Program Files\FileZilla FTP Client\filezilla.exe",
        r"C:\Program Files (x86)\FileZilla FTP Client\filezilla.exe",
    ];
    for p in filezilla_candidates {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return DetectedSftp::FileZilla(pb);
        }
    }

    DetectedSftp::OpenSshSftp
}

pub fn download_bundled_tools() -> anyhow::Result<String> {
    let dir = tools_dir();
    fs::create_dir_all(&dir)?;

    let ala_dest = bundled_alacritty_path();
    if !ala_dest.exists() {
        let status = Command::new("curl.exe")
            .args(["-L", "-s", "-S", "--fail", ALACRITTY_URL, "-o"])
            .arg(&ala_dest)
            .creation_flags(CREATE_NO_WINDOW)
            .status()?;
        if !status.success() {
            let _ = fs::remove_file(&ala_dest);
            anyhow::bail!("Tải Alacritty thất bại (curl exit {})", status);
        }
    }

    let winscp_dest = bundled_winscp_path();
    if !winscp_dest.exists() {
        let zip_path = dir.join("winscp_portable.zip");
        // WinSCP download endpoint or SourceForge mirror
        let urls = [
            "https://sourceforge.net/projects/winscp/files/WinSCP/6.7.1/WinSCP-6.7.1-Portable.zip/download",
            WINSCP_URL,
        ];
        let mut downloaded = false;
        for u in urls {
            let status = Command::new("curl.exe")
                .args(["-L", "-s", "-S", "--fail", "-A", "Wget/1.21.4", u, "-o"])
                .arg(&zip_path)
                .creation_flags(CREATE_NO_WINDOW)
                .status()?;
            if status.success()
                && fs::metadata(&zip_path).map(|m| m.len() > 1_000_000).unwrap_or(false)
            {
                downloaded = true;
                break;
            }
        }
        if !downloaded {
            let _ = fs::remove_file(&zip_path);
            anyhow::bail!("Tải WinSCP Portable thất bại");
        }
        let extract_status = Command::new("tar.exe")
            .args(["-xf"])
            .arg(&zip_path)
            .arg("-C")
            .arg(&dir)
            .creation_flags(CREATE_NO_WINDOW)
            .status()?;
        let _ = fs::remove_file(&zip_path);
        if !extract_status.success() || !winscp_dest.exists() {
            anyhow::bail!("Giải nén WinSCP Portable thất bại");
        }
    }

    Ok(format!("Đã tải sẵn Alacritty & WinSCP vào {}", dir.display()))
}

pub fn build_ssh_args(profile: &SshProfile) -> Vec<String> {
    let mut args = Vec::new();
    args.push("-p".to_string());
    args.push(profile.port.to_string());

    if let AuthMethod::PrivateKey { key_path, .. } = &profile.auth {
        let k = key_path.trim();
        if !k.is_empty() {
            args.push("-i".to_string());
            args.push(k.to_string());
        }
    }

    args.push(format!("{}@{}", profile.username.trim(), profile.host.trim()));
    args
}

pub fn format_ssh_command_line(profile: &SshProfile) -> String {
    let args = build_ssh_args(profile);
    format!("ssh {}", args.join(" "))
}

pub fn build_sftp_url(profile: &SshProfile, include_password: bool) -> String {
    let user = urlencoding_simple(profile.username.trim());
    let pass_part = if include_password {
        if let AuthMethod::Password { password } = &profile.auth {
            if !password.is_empty() {
                format!(":{}", urlencoding_simple(password))
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let path = profile.initial_remote_path.trim();
    let path_suffix = if path.is_empty() || path == "." {
        String::new()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    };

    format!(
        "sftp://{}{}@{}:{}{}",
        user,
        pass_part,
        profile.host.trim(),
        profile.port,
        path_suffix
    )
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn launch_external_terminal(
    profile: &SshProfile,
    settings: &AppSettings,
) -> anyhow::Result<String> {
    let title = format!("[{}] {} — {}@{}", profile.env_badge, profile.name, profile.username.trim(), profile.host.trim());
    let hex_color = format!(
        "#{:02X}{:02X}{:02X}",
        profile.color.r, profile.color.g, profile.color.b
    );
    let ssh_args = build_ssh_args(profile);

    let resolved = match &settings.terminal_app {
        TerminalAppKind::Auto => detect_terminal(),
        TerminalAppKind::Alacritty => {
            let p = bundled_alacritty_path();
            if p.exists() {
                DetectedTerminal::Alacritty(p)
            } else {
                anyhow::bail!("Chưa có Alacritty trong thư mục tools/. Nhấn 'Tải bộ công cụ' để tải về.");
            }
        }
        TerminalAppKind::WindowsTerminal => DetectedTerminal::WindowsTerminal,
        TerminalAppKind::PowerShell => DetectedTerminal::PowerShell,
        TerminalAppKind::Custom(custom_cmd) => {
            let template = custom_cmd.trim();
            if template.is_empty() {
                anyhow::bail!("Chưa cấu hình lệnh Terminal tùy chỉnh.");
            }
            let key_path = match &profile.auth {
                AuthMethod::PrivateKey { key_path, .. } => key_path.trim(),
                _ => "",
            };
            let expanded = template
                .replace("{host}", profile.host.trim())
                .replace("{port}", &profile.port.to_string())
                .replace("{user}", profile.username.trim())
                .replace("{key}", key_path)
                .replace("{title}", &title);

            Command::new("cmd.exe")
                .args(["/c", "start", "", &expanded])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()?;
            return Ok(format!("Đã mở Terminal tùy chỉnh ({})", profile.name));
        }
    };

    match resolved {
        DetectedTerminal::Alacritty(ala_exe) => {
            let ssh_exe = if Path::new(r"C:\Windows\System32\OpenSSH\ssh.exe").exists() {
                r"C:\Windows\System32\OpenSSH\ssh.exe"
            } else {
                "ssh"
            };
            let mut cmd = Command::new(&ala_exe);
            cmd.arg("--title")
                .arg(&title)
                .arg("-o")
                .arg(format!("colors.cursor.cursor=\"{}\"", hex_color))
                .arg("-e")
                .arg(ssh_exe);
            for a in &ssh_args {
                cmd.arg(a);
            }
            cmd.spawn()?;
            Ok(format!("Đã mở Alacritty Terminal ({})", profile.name))
        }
        DetectedTerminal::WindowsTerminal => {
            let mut cmd = Command::new("wt.exe");
            cmd.arg("-w")
                .arg("0")
                .arg("new-tab")
                .arg("--title")
                .arg(&title)
                .arg("--tabColor")
                .arg(&hex_color)
                .arg("ssh");
            for a in &ssh_args {
                cmd.arg(a);
            }
            cmd.spawn()?;
            Ok(format!("Đã mở Windows Terminal ({})", profile.name))
        }
        DetectedTerminal::PowerShell => {
            let ssh_line = format_ssh_command_line(profile);
            let ps_script = format!(
                "$Host.UI.RawUI.WindowTitle = '{}'; {}",
                title.replace('\'', "''"),
                ssh_line
            );
            Command::new("powershell.exe")
                .args(["-NoExit", "-Command", &ps_script])
                .spawn()?;
            Ok(format!("Đã mở PowerShell SSH ({})", profile.name))
        }
    }
}

pub fn launch_external_sftp(
    profile: &SshProfile,
    settings: &AppSettings,
) -> anyhow::Result<String> {
    let title = format!("[{}] {} (SFTP)", profile.env_badge, profile.name);

    let resolved = match &settings.sftp_app {
        SftpAppKind::Auto => detect_sftp(),
        SftpAppKind::WinSCP => {
            let bundled = bundled_winscp_path();
            if bundled.exists() {
                DetectedSftp::WinScp(bundled)
            } else {
                let sys = [
                    r"C:\Program Files (x86)\WinSCP\WinSCP.exe",
                    r"C:\Program Files\WinSCP\WinSCP.exe",
                ]
                .into_iter()
                .map(PathBuf::from)
                .find(|p| p.exists())
                .unwrap_or_else(|| PathBuf::from("WinSCP.exe"));
                DetectedSftp::WinScp(sys)
            }
        }
        SftpAppKind::FileZilla => {
            let sys = [
                r"C:\Program Files\FileZilla FTP Client\filezilla.exe",
                r"C:\Program Files (x86)\FileZilla FTP Client\filezilla.exe",
            ]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from("filezilla.exe"));
            DetectedSftp::FileZilla(sys)
        }
        SftpAppKind::OpenSshSftp => DetectedSftp::OpenSshSftp,
        SftpAppKind::Custom(custom_cmd) => {
            let template = custom_cmd.trim();
            if template.is_empty() {
                anyhow::bail!("Chưa cấu hình lệnh SFTP tùy chỉnh.");
            }
            let url = build_sftp_url(profile, false);
            let expanded = template
                .replace("{url}", &url)
                .replace("{host}", profile.host.trim())
                .replace("{port}", &profile.port.to_string())
                .replace("{user}", profile.username.trim());
            Command::new("cmd.exe")
                .args(["/c", "start", "", &expanded])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()?;
            return Ok(format!("Đã mở SFTP tùy chỉnh ({})", title));
        }
    };

    match resolved {
        DetectedSftp::WinScp(exe) => {
            let url = build_sftp_url(profile, true);
            let mut cmd = Command::new(&exe);
            cmd.arg(&url).arg("/ini=nul");
            if let AuthMethod::PrivateKey { key_path, .. } = &profile.auth {
                if !key_path.trim().is_empty() {
                    cmd.arg(format!("/privatekey={}", key_path.trim()));
                }
            }
            cmd.spawn()?;
            Ok(format!("Đã mở WinSCP SFTP ({})", profile.name))
        }
        DetectedSftp::FileZilla(exe) => {
            let url = build_sftp_url(profile, true);
            Command::new(&exe).arg(&url).spawn()?;
            Ok(format!("Đã mở FileZilla ({})", profile.name))
        }
        DetectedSftp::OpenSshSftp => {
            let mut args = vec!["-P".to_string(), profile.port.to_string()];
            if let AuthMethod::PrivateKey { key_path, .. } = &profile.auth {
                if !key_path.trim().is_empty() {
                    args.push("-i".to_string());
                    args.push(key_path.trim().to_string());
                }
            }
            args.push(format!("{}@{}", profile.username.trim(), profile.host.trim()));
            let sftp_line = format!("sftp {}", args.join(" "));
            let ps_script = format!(
                "$Host.UI.RawUI.WindowTitle = '{}'; {}",
                title.replace('\'', "''"),
                sftp_line
            );
            Command::new("powershell.exe")
                .args(["-NoExit", "-Command", &ps_script])
                .spawn()?;
            Ok(format!("Đã mở OpenSSH SFTP ({})", profile.name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SshProfile;

    #[test]
    fn test_ssh_command_formatting() {
        let mut p = SshProfile::new("Prod Web", "10.0.0.5", "deploy");
        p.port = 2222;
        p.auth = AuthMethod::PrivateKey {
            key_path: "C:\\keys\\id_ed25519".to_string(),
            passphrase: None,
        };
        assert_eq!(
            format_ssh_command_line(&p),
            "ssh -p 2222 -i C:\\keys\\id_ed25519 deploy@10.0.0.5"
        );
    }

    #[test]
    fn test_sftp_url_building() {
        let mut p = SshProfile::new("Staging", "192.168.1.20", "user@domain");
        p.port = 22;
        p.initial_remote_path = "/var/www/app".to_string();
        p.auth = AuthMethod::Password {
            password: "p@ss#1".to_string(),
        };
        assert_eq!(
            build_sftp_url(&p, true),
            "sftp://user%40domain:p%40ss%231@192.168.1.20:22/var/www/app"
        );
        assert_eq!(
            build_sftp_url(&p, false),
            "sftp://user%40domain@192.168.1.20:22/var/www/app"
        );
    }
}
