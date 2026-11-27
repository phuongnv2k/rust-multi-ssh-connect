# Multi-SSH Connect

**Modern Multi-Profile SSH Tunnel, Port Forwarding, Terminal & SFTP Manager for Windows.**  
*Trình quản lý nhiều kết nối SSH, Port Forwarding, Terminal và SFTP song song — gọn nhẹ, trực quan, tối giản.*

[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-blue.svg?logo=windows)](https://www.microsoft.com/windows)
[![UI](https://img.shields.io/badge/UI-WebView2%20(tao%20%2B%20wry)-0078D4.svg)](https://github.com/tauri-apps/wry)
[![Encryption](https://img.shields.io/badge/Backup-AES--256--GCM-green.svg)](https://en.wikipedia.org/wiki/Galois/Counter_Mode)

> **Language / Ngôn ngữ:** [English](#english) · [Tiếng Việt](#tiếng-việt)

---

<a name="english"></a>
## English

### Overview

**Multi-SSH Connect** is a lightweight desktop SSH manager written in Rust. Designed for developers, DevOps engineers, and sysadmins working across multiple environments (`PROD`, `STAGING`, `DEV`, `DB`), it keeps multiple SSH sessions and port-forwarding tunnels active simultaneously with instant switching, clear visual separation, and seamless integration with best-in-class Terminal and SFTP clients.

### Key Features

- **Concurrent Multi-Profile Sessions**: Run multiple SSH tunnels and local port-forwarding rules simultaneously on an async `tokio` + `russh` runtime, with instant tab switching and live status toggles.
- **Visual Environment Distinction**: Color-coded profile cards and environment badges (`PROD`, `STAGING`, `DEV`, `DB`, `PROXY`) keep every server clearly identifiable at a glance.
- **Port Forwarding (`-L`) & Conflict Guard**:
  - Real-time monitoring of listening status, active/total connections, and `TX / RX` traffic, plus quick runtime rule creation.
  - Automated detection of local port overlaps across saved profiles, active sessions, wildcard binds (`0.0.0.0`, `::`, `localhost`, `127.0.0.1`), and OS sockets.
- **Built-in SSH Test Connection**: Verify TCP reachability, SSH handshake, and password or private-key authentication with round-trip latency (`ms`) directly from the profile editor or workspace.
- **Portable Terminal & SFTP Integration**:
  - Self-contained `tools/` directory with 1-click setup for **Alacritty Portable** (Terminal) and **WinSCP Portable** (dual-pane SFTP).
  - Also supports **Windows Terminal (`wt.exe`)** with per-profile tab colors, **PowerShell + OpenSSH**, **FileZilla**, **OpenSSH SFTP**, and custom command templates.
- **Encrypted Backup (Import / Export)**:
  - Export and restore all profiles and settings in an encrypted `.mssh` file protected by **AES-256-GCM** and **PBKDF2-HMAC-SHA256** (100,000 iterations, random salt and nonce), supporting **Merge** and **Replace** modes.
- **Minimalist Light & Dark Themes**: Native Windows **WebView2** (`tao` + `wry`) interface with crisp typography and SVG iconography.

### Tech Stack

- **Core**: Rust (`tokio`, `russh`, `russh-keys`)
- **Desktop UI**: `tao` + `wry` (Windows Edge WebView2)
- **Cryptography**: `aes-gcm`, `pbkdf2`, `hmac`, `sha2`
- **Configuration**: JSON storage in `%APPDATA%\rust-multi-ssh\RustMultiSshConnect\config\`

### Build & Test (Windows)

```powershell
# Run unit tests
cargo test --target x86_64-pc-windows-gnullvm

# Build optimized release binary
cargo build --release --target x86_64-pc-windows-gnullvm
```

Release binary: `target/x86_64-pc-windows-gnullvm/release/rust-multi-ssh-connect.exe`

---

<a name="tiếng-việt"></a>
## Tiếng Việt

### Giới thiệu

**Multi-SSH Connect** là ứng dụng quản lý kết nối SSH trên Windows được viết bằng Rust. Ứng dụng giúp lập trình viên, DevOps và quản trị hệ thống vận hành đồng thời nhiều server (`PROD`, `STAGING`, `DEV`, `DB`), chuyển đổi nhanh giữa các phiên đang chạy, theo dõi Port Forwarding trực quan và mở nhanh Terminal / SFTP chuyên dụng.

### Tính năng chính

- **Vận hành nhiều SSH Profile song song**: Giữ nhiều tunnel SSH và rule Port Forwarding hoạt động đồng thời trên nền `tokio` + `russh`, bật/tắt nhanh bằng công tắc trực tiếp trên từng profile và chuyển tab tức thì.
- **Phân biệt môi trường rõ ràng**: Mỗi profile có dải màu nhận diện và nhãn môi trường (`PROD`, `STAGING`, `DEV`, `DB`, `PROXY`) giúp dễ dàng phân biệt khi làm việc với nhiều máy chủ cùng lúc.
- **Port Forwarding (`-L`) & Kiểm soát xung đột**:
  - Theo dõi trạng thái `Listening`, số kết nối `active / total`, lưu lượng `↑ TX / ↓ RX`, và thêm nhanh rule ngay khi phiên đang chạy.
  - Tự động phát hiện và cảnh báo khi các profile dùng chung local port (bao gồm `0.0.0.0`, `::`, `localhost`, `127.0.0.1` và port hệ thống).
- **Test Connection (Kiểm tra kết nối SSH)**: Kiểm tra nhanh kết nối TCP, SSH handshake, xác thực mật khẩu/SSH key và đo độ trễ (`ms`) ngay trong trình chỉnh sửa hoặc màn hình chi tiết profile.
- **Tích hợp Terminal & SFTP Portable**:
  - Tải và quản lý **Alacritty Portable** (Terminal) cùng **WinSCP Portable** (SFTP) trong thư mục `tools/` độc lập cạnh file thực thi.
  - Hỗ trợ **Windows Terminal (`wt.exe`)** với màu tab theo profile, **PowerShell + OpenSSH**, **FileZilla**, **OpenSSH SFTP**, hoặc lệnh tùy chỉnh.
- **Nhập / Xuất dữ liệu mã hoá**:
  - Sao lưu và khôi phục toàn bộ Profile và Cài đặt qua file `.mssh` được mã hoá bằng **AES-256-GCM** và **PBKDF2-HMAC-SHA256** (100.000 vòng), hỗ trợ chế độ **Gộp thêm (Merge)** hoặc **Thay thế (Replace)**.
- **Giao diện Sáng / Tối tối giản**: Dựng trên **WebView2** (`tao` + `wry`) với font chữ sắc nét và đầy đủ tiếng Việt.

### Hướng dẫn Build & Test

```powershell
# Chạy toàn bộ unit test
cargo test --target x86_64-pc-windows-gnullvm

# Build bản release tối ưu
cargo build --release --target x86_64-pc-windows-gnullvm
```

File thực thi sau khi build: `target/x86_64-pc-windows-gnullvm/release/rust-multi-ssh-connect.exe`

---

### Topics

`rust` `ssh-client` `ssh-manager` `multi-ssh` `port-forwarding` `ssh-tunnel` `sftp` `winscp` `alacritty` `windows-ssh` `webview2` `wry` `tao` `russh` `aes-256-gcm`
