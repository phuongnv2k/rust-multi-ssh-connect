# Multi-SSH Connect

**Minimalist Multi-Profile SSH Tunnel, Port Forwarding, Terminal & SFTP Manager built in Rust for Windows.**  
*Trình quản lý nhiều kết nối SSH, Port Forwarding, Terminal và SFTP song song — tối giản, nhẹ, thay thế Bitvise SSH Client.*

[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-blue.svg?logo=windows)](https://www.microsoft.com/windows)
[![UI](https://img.shields.io/badge/UI-WebView2%20(tao%20%2B%20wry)-0078D4.svg)](https://github.com/tauri-apps/wry)
[![Crypto](https://img.shields.io/badge/Backup-AES--256--GCM-green.svg)](https://en.wikipedia.org/wiki/Galois/Counter_Mode)

> **Language / Ngôn ngữ:** [English](#english) · [Tiếng Việt](#tiếng-việt)

---

<a name="english"></a>
## English

### Why Multi-SSH Connect?

Traditional SSH clients like Bitvise SSH Client force you to reload profiles every time you switch servers, making it painful to work across `PROD`, `STAGING`, and `DEV` environments simultaneously.

**Multi-SSH Connect** lets you run **multiple SSH profiles concurrently** with zero reload, unmistakable visual color-coding, automatic **local port conflict blocking**, 1-click **Test Connection**, portable **Alacritty + WinSCP** integration, and **AES-256-GCM encrypted** backup/restore.

### Key Features

- **Concurrent Multi-Profile Live Sessions**: Keep multiple SSH tunnels and port-forwarding rules running simultaneously in the background (`tokio` + `russh`) and switch between them in 1 click.
- **Unmistakable Visual Profile Identity**: Assign a custom **accent color**, **2-letter monogram**, and **environment badge** (`PROD`, `STAGING`, `DEV`, `DB`, `PROXY`) to every server so you never mix up production and staging.
- **Smart Port Forwarding (`-L`) & Conflict Guard**:
  - Live runtime stats (`Listening`, active/total connections, `TX / RX` bytes) and quick-add forwarding rules.
  - **Early Overlap Warning** in the sidebar when two saved profiles share a local port.
  - **Hard Conflict Blocking** before connecting or toggling a rule if another active profile or OS process already binds that port (handles `0.0.0.0`, `::`, `localhost`, and `127.0.0.1` collisions).
- **1-Click SSH Test Connection**: Verify TCP reachability, SSH handshake, and password/private-key authentication with latency measurement (`ms`) before saving or connecting.
- **3rd-Party Terminal & SFTP (Zero Console Flash)**:
  - Dedicated portable `tools/` folder next to the executable with 1-click download for **Alacritty Portable** (Terminal) and **WinSCP Portable** (dual-pane SFTP).
  - Also supports **Windows Terminal (`wt.exe`)** with per-profile tab colors, **PowerShell + OpenSSH**, **FileZilla**, **OpenSSH SFTP**, or custom commands.
- **Password-Protected Backup (Import / Export)**:
  - Export all profiles (passwords, key paths, passphrases, port-forward rules) and settings into an encrypted `.mssh` file using **AES-256-GCM** + **PBKDF2-HMAC-SHA256** (100,000 iterations, random salt & nonce).
  - Import with **Merge** or **Replace** mode.
- **Minimalist Light & Dark UI**: Powered by native Windows **WebView2** (`tao` + `wry`) with clean typography and crisp SVG icons.

### Tech Stack

- **Core**: Rust (`tokio`, `russh`, `russh-keys`)
- **Desktop Window & UI**: `tao` + `wry` (Windows Edge WebView2) + embedded HTML/CSS/JS
- **Encryption**: `aes-gcm`, `pbkdf2`, `hmac`, `sha2`
- **Storage**: JSON config in `%APPDATA%\rust-multi-ssh\RustMultiSshConnect\config\`

### Build & Test (Windows)

```powershell
# Run unit tests
cargo test --target x86_64-pc-windows-gnullvm

# Build optimized release binary
cargo build --release --target x86_64-pc-windows-gnullvm
```

Output executable: `target/x86_64-pc-windows-gnullvm/release/rust-multi-ssh-connect.exe`

---

<a name="tiếng-việt"></a>
## Tiếng Việt

### Multi-SSH Connect giải quyết vấn đề gì?

Khi dùng Bitvise SSH Client để quản lý nhiều server, mỗi lần chuyển server bạn phải load lại profile rất bất tiện và không thể chạy song song nhiều kết nối cùng lúc.

**Multi-SSH Connect** được viết bằng Rust giúp bạn **bật nhiều profile SSH cùng lúc** không cần reload, **phân biệt màu sắc & nhãn môi trường** rõ ràng, **cảnh báo & chặn xung đột port tự động**, **Test Connection** nhanh, mở **Alacritty / WinSCP** từ thư mục `tools/` riêng (không nháy cửa sổ command), và **nhập/xuất dữ liệu mã hoá mật khẩu**.

### Tính năng nổi bật

- **Chạy nhiều SSH Profile song song (Live)**: Giữ nhiều tunnel SSH và rule Port Forwarding hoạt động cùng lúc trên nền `tokio` + `russh`, chuyển tab tức thì không ngắt kết nối.
- **Phân biệt Profile trực quan**: Mỗi server có **màu nhận diện riêng**, **avatar 2 chữ cái**, và **nhãn môi trường** (`PROD`, `STAGING`, `DEV`, `DB`, `PROXY`) giúp tránh thao tác nhầm giữa các môi trường.
- **Port Forwarding (`-L`) & Chống xung đột Port**:
  - Theo dõi trạng thái `Listening`, số kết nối `active / total`, lưu lượng `↑ TX / ↓ RX`, và thêm nhanh rule khi đang chạy.
  - **Cảnh báo sớm** ở danh sách nếu 2 profile đã lưu cấu hình trùng port.
  - **Chặn cứng** trước khi bật Live hoặc bật rule nếu port đang bị profile khác hoặc tiến trình hệ thống chiếm dụng (kiểm tra cả `0.0.0.0`, `::`, `localhost`, `127.0.0.1` và `TcpListener::bind`).
- **Test Connection (Kiểm tra kết nối SSH)**: Kiểm tra nhanh TCP + handshake SSH + xác thực mật khẩu/SSH key và đo độ trễ (`ms`) ngay trong lúc tạo/sửa hoặc trên thẻ Profile.
- **Mở Terminal & SFTP bên thứ 3 (Không nháy CMD)**:
  - Tự quản lý bộ công cụ Portable trong thư mục `tools/` cạnh file `.exe` với nút tải nhanh **Alacritty Portable** (Terminal) và **WinSCP Portable** (SFTP).
  - Hỗ trợ thêm **Windows Terminal (`wt.exe`)** tô màu tab theo profile, **PowerShell + OpenSSH**, **FileZilla**, **OpenSSH SFTP**, hoặc lệnh tùy chỉnh.
- **Nhập / Xuất dữ liệu bảo vệ bằng mật khẩu**:
  - Xuất toàn bộ Profile & Cài đặt ra file `.mssh` được mã hoá **AES-256-GCM** + **PBKDF2-HMAC-SHA256** (100.000 vòng, salt & nonce ngẫu nhiên).
  - Hỗ trợ nhập lại theo chế độ **Gộp thêm (Merge)** hoặc **Thay thế toàn bộ (Replace)**.
- **Giao diện Sáng / Tối tối giản**: Dựng trên **WebView2** (`tao` + `wry`) hỗ trợ đầy đủ font tiếng Việt sắc nét.

### Hướng dẫn Build & Test

```powershell
# Chạy toàn bộ unit test
cargo test --target x86_64-pc-windows-gnullvm

# Build bản release tối ưu
cargo build --release --target x86_64-pc-windows-gnullvm
```

File thực thi sau khi build: `target/x86_64-pc-windows-gnullvm/release/rust-multi-ssh-connect.exe`

---

### Keywords / GitHub Topics

`rust` `ssh-client` `ssh-manager` `multi-ssh` `port-forwarding` `ssh-tunnel` `sftp` `winscp` `alacritty` `bitvise-alternative` `windows-ssh` `webview2` `wry` `tao` `russh` `aes-256-gcm`
