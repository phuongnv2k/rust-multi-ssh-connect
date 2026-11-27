#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod launcher;
mod models;
mod port_guard;
mod ssh_manager;

use crate::launcher::{
    detect_sftp, detect_terminal, download_bundled_tools, format_ssh_command_line,
    has_bundled_tools, launch_external_sftp, launch_external_terminal, tools_dir, DetectedSftp,
    DetectedTerminal,
};
use crate::models::{
    AppSettings, AuthMethod, PortForwardRule, ProfileColor, ProfileStore, SftpAppKind,
    SshProfile, TerminalAppKind,
};
use crate::port_guard::{
    check_activation_conflicts, check_saved_overlaps, check_single_rule_conflict,
    ActivePortBinding, PortConflict,
};
use crate::ssh_manager::{
    ConnectionStatus, ForwardRuntimeState, ForwardStatus, SessionCommand, SessionEvent,
    SshRuntime,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use tokio::sync::mpsc;
use uuid::Uuid;
use wry::WebViewBuilder;

enum AppEvent {
    Ipc(String),
    Session(SessionEvent),
    TestResult {
        is_draft: bool,
        result: Result<u128, String>,
    },
    ToolsDownloaded(Result<String, String>),
}

struct ActiveSession {
    profile: SshProfile,
    status: ConnectionStatus,
    cmd_tx: mpsc::UnboundedSender<SessionCommand>,
    forwards: HashMap<Uuid, ForwardRuntimeState>,
}

impl ActiveSession {
    fn new(
        profile: SshProfile,
        allowed_forwards: Vec<PortForwardRule>,
        cmd_tx: mpsc::UnboundedSender<SessionCommand>,
    ) -> Self {
        let allowed_ids: HashSet<Uuid> = allowed_forwards.iter().map(|r| r.id).collect();
        let mut forwards = HashMap::new();
        for rule in &profile.port_forwards {
            let status = if allowed_ids.contains(&rule.id) {
                ForwardStatus::Starting
            } else {
                ForwardStatus::Stopped
            };
            forwards.insert(
                rule.id,
                ForwardRuntimeState {
                    rule: rule.clone(),
                    status,
                    active_conns: 0,
                    total_conns: 0,
                    bytes_tx: 0,
                    bytes_rx: 0,
                },
            );
        }
        Self {
            profile,
            status: ConnectionStatus::Connecting,
            cmd_tx,
            forwards,
        }
    }
}

#[derive(Serialize)]
struct UiSession {
    profile: SshProfile,
    status: String,
    forwards: HashMap<Uuid, UiForward>,
}

#[derive(Serialize)]
struct UiForward {
    rule: PortForwardRule,
    status: String,
    active_conns: u64,
    total_conns: u64,
    bytes_tx: u64,
    bytes_rx: u64,
}

#[derive(Serialize)]
struct UiOverlap {
    desc: String,
}

#[derive(Serialize)]
struct UiNotice {
    is_error: bool,
    message: String,
}

#[derive(Serialize)]
struct UiConflictItem {
    rule_id: Uuid,
    desc: String,
}

#[derive(Serialize)]
struct UiConflictModal {
    title: String,
    profile_id: Uuid,
    profile_name: String,
    profile_env: String,
    allow_skip: bool,
    conflicts: Vec<UiConflictItem>,
}

#[derive(Serialize)]
struct UiState {
    profiles: Vec<SshProfile>,
    sessions: HashMap<Uuid, UiSession>,
    tab_order: Vec<Uuid>,
    selected_id: Option<Uuid>,
    overlaps: HashMap<Uuid, Vec<UiOverlap>>,
    settings: AppSettings,
    detected_terminal: String,
    detected_sftp: String,
    tools_alacritty_ok: bool,
    tools_winscp_ok: bool,
    tools_dir: String,
    notice: Option<UiNotice>,
    conflict_modal: Option<UiConflictModal>,
}

#[derive(Deserialize)]
struct IncomingRule {
    id: Option<Uuid>,
    enabled: bool,
    label: String,
    bind_address: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
}

#[derive(Deserialize)]
struct IncomingProfile {
    id: Option<Uuid>,
    name: String,
    env_badge: String,
    color: ProfileColor,
    host: String,
    port: u16,
    username: String,
    auth: AuthMethod,
    initial_remote_path: String,
    port_forwards: Vec<IncomingRule>,
    notes: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum IpcMsg {
    Init,
    ToggleTheme,
    SelectProfile {
        id: Uuid,
    },
    SaveProfile {
        profile: IncomingProfile,
    },
    DuplicateProfile {
        id: Uuid,
    },
    DeleteProfile {
        id: Uuid,
    },
    Connect {
        id: Uuid,
    },
    ConnectSkipConflicts {
        id: Uuid,
        bad_rule_ids: Vec<Uuid>,
    },
    Disconnect {
        id: Uuid,
    },
    CloseTab {
        id: Uuid,
    },
    ToggleForward {
        profile_id: Uuid,
        rule_id: Uuid,
        enable: bool,
    },
    AddQuickForward {
        profile_id: Uuid,
        label: String,
        bind_address: String,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    LaunchTerminal {
        id: Uuid,
    },
    LaunchSftp {
        id: Uuid,
    },
    CopySsh {
        id: Uuid,
    },
    TestConnection {
        id: Uuid,
    },
    TestDraftConnection {
        profile: IncomingProfile,
    },
    PickKeyFile,
    SaveSettings {
        terminal_app: TerminalAppKind,
        sftp_app: SftpAppKind,
    },
    DownloadTools,
    ExportData {
        password: String,
    },
    ImportData {
        password: String,
        replace_all: bool,
    },
    DismissConflict,
}

struct BackendState {
    store: ProfileStore,
    settings: AppSettings,
    runtime: SshRuntime,
    sessions: HashMap<Uuid, ActiveSession>,
    tab_order: Vec<Uuid>,
    selected_id: Option<Uuid>,
    overlaps: HashMap<Uuid, Vec<PortConflict>>,
    notice: Option<UiNotice>,
    conflict_modal: Option<UiConflictModal>,
    proxy: EventLoopProxy<AppEvent>,
}

impl BackendState {
    fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        let store = ProfileStore::load_default();
        let settings = AppSettings::load_default();
        let overlaps = check_saved_overlaps(&store.profiles);
        let selected_id = store.profiles.first().map(|p| p.id);
        let runtime = SshRuntime::new().expect("Failed to initialize Tokio runtime");
        Self {
            store,
            settings,
            runtime,
            sessions: HashMap::new(),
            tab_order: Vec::new(),
            selected_id,
            overlaps,
            notice: None,
            conflict_modal: None,
            proxy,
        }
    }

    fn active_bindings(&self) -> Vec<ActivePortBinding> {
        let mut out = Vec::new();
        for s in self.sessions.values() {
            if matches!(
                s.status,
                ConnectionStatus::Disconnected | ConnectionStatus::Error(_)
            ) {
                continue;
            }
            for fw in s.forwards.values() {
                if matches!(
                    fw.status,
                    ForwardStatus::Starting | ForwardStatus::Listening
                ) {
                    out.push(ActivePortBinding {
                        profile_id: s.profile.id,
                        profile_name: s.profile.name.clone(),
                        env_badge: s.profile.env_badge.clone(),
                        rule_id: fw.rule.id,
                        rule_label: fw.rule.label.clone(),
                        bind_address: fw.rule.normalized_bind(),
                        local_port: fw.rule.local_port,
                    });
                }
            }
        }
        out
    }

    fn launch_session(&mut self, profile: SshProfile, allowed: Vec<PortForwardRule>) {
        let id = profile.id;
        let proxy = self.proxy.clone();
        let cb: ssh_manager::EventCallback = Arc::new(move |ev| {
            let _ = proxy.send_event(AppEvent::Session(ev));
        });
        let cmd_tx = self
            .runtime
            .spawn_session(profile.clone(), allowed.clone(), cb);
        let session = ActiveSession::new(profile, allowed, cmd_tx);
        self.sessions.insert(id, session);
        if !self.tab_order.contains(&id) {
            self.tab_order.push(id);
        }
        self.selected_id = Some(id);
    }

    fn build_ui_state(&self) -> UiState {
        let mut ui_sessions = HashMap::new();
        for (id, s) in &self.sessions {
            let status = match &s.status {
                ConnectionStatus::Connecting => "Connecting".to_string(),
                ConnectionStatus::Connected => "Connected".to_string(),
                ConnectionStatus::Disconnected => "Disconnected".to_string(),
                ConnectionStatus::Error(e) => format!("Error: {}", e),
            };
            let mut forwards = HashMap::new();
            for (rid, fw) in &s.forwards {
                let fst = match &fw.status {
                    ForwardStatus::Starting => "Starting".to_string(),
                    ForwardStatus::Listening => "Listening".to_string(),
                    ForwardStatus::Stopped => "Stopped".to_string(),
                    ForwardStatus::Error(e) => format!("Error: {}", e),
                };
                forwards.insert(
                    *rid,
                    UiForward {
                        rule: fw.rule.clone(),
                        status: fst,
                        active_conns: fw.active_conns,
                        total_conns: fw.total_conns,
                        bytes_tx: fw.bytes_tx,
                        bytes_rx: fw.bytes_rx,
                    },
                );
            }
            ui_sessions.insert(
                *id,
                UiSession {
                    profile: s.profile.clone(),
                    status,
                    forwards,
                },
            );
        }

        let mut ui_overlaps = HashMap::new();
        for (id, list) in &self.overlaps {
            ui_overlaps.insert(
                *id,
                list.iter()
                    .map(|c| UiOverlap { desc: c.describe() })
                    .collect(),
            );
        }

        let det_term = match &self.settings.terminal_app {
            TerminalAppKind::Auto => match detect_terminal() {
                DetectedTerminal::Alacritty(_) => "Alacritty".to_string(),
                DetectedTerminal::WindowsTerminal => "Windows Terminal".to_string(),
                DetectedTerminal::PowerShell => "PowerShell".to_string(),
            },
            TerminalAppKind::Alacritty => "Alacritty".to_string(),
            TerminalAppKind::WindowsTerminal => "Windows Terminal".to_string(),
            TerminalAppKind::PowerShell => "PowerShell".to_string(),
            TerminalAppKind::Custom(_) => "Tùy chỉnh".to_string(),
        };

        let det_sftp = match &self.settings.sftp_app {
            SftpAppKind::Auto => match detect_sftp() {
                DetectedSftp::WinScp(_) => "WinSCP".to_string(),
                DetectedSftp::FileZilla(_) => "FileZilla".to_string(),
                DetectedSftp::OpenSshSftp => "OpenSSH".to_string(),
            },
            SftpAppKind::WinSCP => "WinSCP".to_string(),
            SftpAppKind::FileZilla => "FileZilla".to_string(),
            SftpAppKind::OpenSshSftp => "OpenSSH".to_string(),
            SftpAppKind::Custom(_) => "Tùy chỉnh".to_string(),
        };

        let (tools_alacritty_ok, tools_winscp_ok) = has_bundled_tools();

        UiState {
            profiles: self.store.profiles.clone(),
            sessions: ui_sessions,
            tab_order: self.tab_order.clone(),
            selected_id: self.selected_id,
            overlaps: ui_overlaps,
            settings: self.settings.clone(),
            detected_terminal: det_term,
            detected_sftp: det_sftp,
            tools_alacritty_ok,
            tools_winscp_ok,
            tools_dir: tools_dir().display().to_string(),
            notice: self.notice.as_ref().map(|n| UiNotice {
                is_error: n.is_error,
                message: n.message.clone(),
            }),
            conflict_modal: self.conflict_modal.as_ref().map(|m| UiConflictModal {
                title: m.title.clone(),
                profile_id: m.profile_id,
                profile_name: m.profile_name.clone(),
                profile_env: m.profile_env.clone(),
                allow_skip: m.allow_skip,
                conflicts: m
                    .conflicts
                    .iter()
                    .map(|c| UiConflictItem {
                        rule_id: c.rule_id,
                        desc: c.desc.clone(),
                    })
                    .collect(),
            }),
        }
    }

    fn handle_ipc(&mut self, raw: &str, webview: &wry::WebView) {
        let Ok(msg) = serde_json::from_str::<IpcMsg>(raw) else {
            return;
        };

        match msg {
            IpcMsg::Init => {}
            IpcMsg::ToggleTheme => {
                self.settings.theme_light = !self.settings.theme_light;
                let _ = self.settings.save();
            }
            IpcMsg::SelectProfile { id } => {
                self.selected_id = Some(id);
                self.notice = None;
            }
            IpcMsg::SaveProfile { profile } => {
                let p = SshProfile {
                    id: profile.id.unwrap_or_else(Uuid::new_v4),
                    name: profile.name,
                    env_badge: profile.env_badge,
                    color: profile.color,
                    host: profile.host,
                    port: profile.port,
                    username: profile.username,
                    auth: profile.auth,
                    initial_remote_path: profile.initial_remote_path,
                    port_forwards: profile
                        .port_forwards
                        .into_iter()
                        .map(|r| PortForwardRule {
                            id: r.id.unwrap_or_else(Uuid::new_v4),
                            enabled: r.enabled,
                            label: r.label,
                            bind_address: r.bind_address,
                            local_port: r.local_port,
                            remote_host: r.remote_host,
                            remote_port: r.remote_port,
                        })
                        .collect(),
                    notes: profile.notes,
                };
                self.selected_id = Some(p.id);
                let _ = self.store.upsert(p);
                self.overlaps = check_saved_overlaps(&self.store.profiles);
            }
            IpcMsg::DuplicateProfile { id } => {
                if let Ok(Some(copy)) = self.store.duplicate(id) {
                    self.selected_id = Some(copy.id);
                    self.overlaps = check_saved_overlaps(&self.store.profiles);
                }
            }
            IpcMsg::DeleteProfile { id } => {
                if let Some(s) = self.sessions.get(&id) {
                    let _ = s.cmd_tx.send(SessionCommand::Disconnect);
                }
                self.sessions.remove(&id);
                self.tab_order.retain(|&x| x != id);
                let _ = self.store.delete(id);
                if self.selected_id == Some(id) {
                    self.selected_id = self.store.profiles.first().map(|p| p.id);
                }
                self.overlaps = check_saved_overlaps(&self.store.profiles);
            }
            IpcMsg::Connect { id } => {
                if let Some(profile) = self.store.profiles.iter().find(|p| p.id == id).cloned() {
                    let active = self.active_bindings();
                    let conflicts = check_activation_conflicts(&profile, &active, true);
                    if !conflicts.is_empty() {
                        self.conflict_modal = Some(UiConflictModal {
                            title: "Phát hiện xung đột Port Forwarding".to_string(),
                            profile_id: profile.id,
                            profile_name: profile.name.clone(),
                            profile_env: profile.env_badge.clone(),
                            allow_skip: true,
                            conflicts: conflicts
                                .into_iter()
                                .map(|c| UiConflictItem {
                                    rule_id: c.rule_id,
                                    desc: c.describe(),
                                })
                                .collect(),
                        });
                    } else {
                        let allowed = profile
                            .enabled_port_forwards()
                            .into_iter()
                            .cloned()
                            .collect();
                        self.launch_session(profile, allowed);
                    }
                }
            }
            IpcMsg::ConnectSkipConflicts { id, bad_rule_ids } => {
                self.conflict_modal = None;
                if let Some(profile) = self.store.profiles.iter().find(|p| p.id == id).cloned() {
                    let bad: HashSet<Uuid> = bad_rule_ids.into_iter().collect();
                    let allowed = profile
                        .enabled_port_forwards()
                        .into_iter()
                        .filter(|r| !bad.contains(&r.id))
                        .cloned()
                        .collect();
                    self.launch_session(profile, allowed);
                }
            }
            IpcMsg::Disconnect { id } => {
                if let Some(s) = self.sessions.get(&id) {
                    let _ = s.cmd_tx.send(SessionCommand::Disconnect);
                }
            }
            IpcMsg::CloseTab { id } => {
                if let Some(s) = self.sessions.get(&id) {
                    let _ = s.cmd_tx.send(SessionCommand::Disconnect);
                }
                self.sessions.remove(&id);
                self.tab_order.retain(|&x| x != id);
            }
            IpcMsg::ToggleForward {
                profile_id,
                rule_id,
                enable,
            } => {
                if !enable {
                    if let Some(s) = self.sessions.get(&profile_id) {
                        let _ = s.cmd_tx.send(SessionCommand::StopForward(rule_id));
                    }
                } else {
                    let active = self.active_bindings();
                    if let Some(s) = self.sessions.get(&profile_id) {
                        if let Some(fw) = s.forwards.get(&rule_id) {
                            let rule = fw.rule.clone();
                            let same: Vec<PortForwardRule> = s
                                .forwards
                                .values()
                                .filter(|f| {
                                    matches!(
                                        f.status,
                                        ForwardStatus::Listening | ForwardStatus::Starting
                                    )
                                })
                                .map(|f| f.rule.clone())
                                .collect();
                            if let Some(conflict) =
                                check_single_rule_conflict(profile_id, &rule, &same, &active, true)
                            {
                                self.conflict_modal = Some(UiConflictModal {
                                    title: "Không thể bật Port Forward do xung đột".to_string(),
                                    profile_id,
                                    profile_name: s.profile.name.clone(),
                                    profile_env: s.profile.env_badge.clone(),
                                    allow_skip: false,
                                    conflicts: vec![UiConflictItem {
                                        rule_id: conflict.rule_id,
                                        desc: conflict.describe(),
                                    }],
                                });
                            } else if let Some(s_mut) = self.sessions.get_mut(&profile_id) {
                                if let Some(fw_mut) = s_mut.forwards.get_mut(&rule_id) {
                                    fw_mut.status = ForwardStatus::Starting;
                                }
                                let _ = s_mut.cmd_tx.send(SessionCommand::StartForward(rule));
                            }
                        }
                    }
                }
            }
            IpcMsg::AddQuickForward {
                profile_id,
                label,
                bind_address,
                local_port,
                remote_host,
                remote_port,
            } => {
                let rule = PortForwardRule::new(label, bind_address, local_port, remote_host, remote_port);
                let rule_id = rule.id;
                let active = self.active_bindings();
                if let Some(s) = self.sessions.get(&profile_id) {
                    let same: Vec<PortForwardRule> = s
                        .forwards
                        .values()
                        .filter(|f| {
                            matches!(
                                f.status,
                                ForwardStatus::Listening | ForwardStatus::Starting
                            )
                        })
                        .map(|f| f.rule.clone())
                        .collect();
                    if let Some(conflict) =
                        check_single_rule_conflict(profile_id, &rule, &same, &active, true)
                    {
                        self.conflict_modal = Some(UiConflictModal {
                            title: "Không thể bật Port Forward do xung đột".to_string(),
                            profile_id,
                            profile_name: s.profile.name.clone(),
                            profile_env: s.profile.env_badge.clone(),
                            allow_skip: false,
                            conflicts: vec![UiConflictItem {
                                rule_id: conflict.rule_id,
                                desc: conflict.describe(),
                            }],
                        });
                    } else if let Some(s_mut) = self.sessions.get_mut(&profile_id) {
                        s_mut.forwards.insert(
                            rule_id,
                            ForwardRuntimeState {
                                rule: rule.clone(),
                                status: ForwardStatus::Starting,
                                active_conns: 0,
                                total_conns: 0,
                                bytes_tx: 0,
                                bytes_rx: 0,
                            },
                        );
                        let _ = s_mut.cmd_tx.send(SessionCommand::StartForward(rule));
                    }
                }
            }
            IpcMsg::LaunchTerminal { id } => {
                if let Some(p) = self.store.profiles.iter().find(|p| p.id == id) {
                    self.notice = match launch_external_terminal(p, &self.settings) {
                        Ok(m) => Some(UiNotice {
                            is_error: false,
                            message: m,
                        }),
                        Err(e) => Some(UiNotice {
                            is_error: true,
                            message: format!("Không mở được Terminal: {}", e),
                        }),
                    };
                }
            }
            IpcMsg::LaunchSftp { id } => {
                if let Some(p) = self.store.profiles.iter().find(|p| p.id == id) {
                    self.notice = match launch_external_sftp(p, &self.settings) {
                        Ok(m) => Some(UiNotice {
                            is_error: false,
                            message: m,
                        }),
                        Err(e) => Some(UiNotice {
                            is_error: true,
                            message: format!("Không mở được SFTP: {}", e),
                        }),
                    };
                }
            }
            IpcMsg::CopySsh { id } => {
                if let Some(p) = self.store.profiles.iter().find(|p| p.id == id) {
                    let cmd = format_ssh_command_line(p);
                    let js = format!(
                        "navigator.clipboard && navigator.clipboard.writeText({});",
                        serde_json::to_string(&cmd).unwrap_or_default()
                    );
                    let _ = webview.evaluate_script(&js);
                    self.notice = Some(UiNotice {
                        is_error: false,
                        message: format!("Đã copy: {}", cmd),
                    });
                }
            }
            IpcMsg::TestConnection { id } => {
                if let Some(p) = self.store.profiles.iter().find(|p| p.id == id).cloned() {
                    self.notice = Some(UiNotice {
                        is_error: false,
                        message: format!("Đang kiểm tra kết nối tới {}:{}...", p.host, p.port),
                    });
                    let proxy = self.proxy.clone();
                    self.runtime.test_connection(
                        p,
                        Arc::new(move |result| {
                            let _ = proxy.send_event(AppEvent::TestResult {
                                is_draft: false,
                                result,
                            });
                        }),
                    );
                }
            }
            IpcMsg::TestDraftConnection { profile } => {
                let p = SshProfile {
                    id: profile.id.unwrap_or_else(Uuid::new_v4),
                    name: profile.name,
                    env_badge: profile.env_badge,
                    color: profile.color,
                    host: profile.host,
                    port: profile.port,
                    username: profile.username,
                    auth: profile.auth,
                    initial_remote_path: profile.initial_remote_path,
                    port_forwards: Vec::new(),
                    notes: String::new(),
                };
                let proxy = self.proxy.clone();
                self.runtime.test_connection(
                    p,
                    Arc::new(move |result| {
                        let _ = proxy.send_event(AppEvent::TestResult {
                            is_draft: true,
                            result,
                        });
                    }),
                );
                return;
            }
            IpcMsg::PickKeyFile => {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    let p_str = path.display().to_string();
                    let js = format!(
                        "window.__onKeyFilePicked({});",
                        serde_json::to_string(&p_str).unwrap_or_default()
                    );
                    let _ = webview.evaluate_script(&js);
                }
                return;
            }
            IpcMsg::SaveSettings {
                terminal_app,
                sftp_app,
            } => {
                self.settings.terminal_app = terminal_app;
                self.settings.sftp_app = sftp_app;
                let _ = self.settings.save();
            }
            IpcMsg::DownloadTools => {
                self.notice = Some(UiNotice {
                    is_error: false,
                    message: "Đang tải Alacritty & WinSCP Portable về thư mục tools/..."
                        .to_string(),
                });
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let res = download_bundled_tools().map_err(|e| e.to_string());
                    let _ = proxy.send_event(AppEvent::ToolsDownloaded(res));
                });
            }
            IpcMsg::ExportData { password } => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("multi-ssh-backup.mssh")
                    .add_filter("Multi-SSH Backup", &["mssh", "json"])
                    .save_file()
                {
                    let res = backup::export_to_file(
                        &path,
                        &self.store.profiles,
                        &self.settings,
                        &password,
                    );
                    let (ok, msg) = match res {
                        Ok(()) => (
                            true,
                            format!(
                                "Đã xuất {} profile (mã hoá AES-256) ra {}",
                                self.store.profiles.len(),
                                path.display()
                            ),
                        ),
                        Err(e) => (false, e.to_string()),
                    };
                    let js = format!(
                        "window.__onBackupResult({}, {});",
                        ok,
                        serde_json::to_string(&msg).unwrap_or_default()
                    );
                    let _ = webview.evaluate_script(&js);
                }
                return;
            }
            IpcMsg::ImportData {
                password,
                replace_all,
            } => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Multi-SSH Backup", &["mssh", "json"])
                    .pick_file()
                {
                    match backup::import_from_file(&path, &password) {
                        Ok(payload) => {
                            let imported_count = payload.profiles.len();
                            if replace_all {
                                for s in self.sessions.values() {
                                    let _ = s.cmd_tx.send(SessionCommand::Disconnect);
                                }
                                self.sessions.clear();
                                self.tab_order.clear();
                                self.store.profiles = payload.profiles;
                            } else {
                                for p in payload.profiles {
                                    if let Some(existing) =
                                        self.store.profiles.iter_mut().find(|x| x.id == p.id)
                                    {
                                        *existing = p;
                                    } else {
                                        self.store.profiles.push(p);
                                    }
                                }
                            }
                            let _ = self.store.save();
                            self.settings = payload.settings;
                            let _ = self.settings.save();
                            self.overlaps = check_saved_overlaps(&self.store.profiles);
                            if self.selected_id.is_none() {
                                self.selected_id = self.store.profiles.first().map(|p| p.id);
                            }
                            let msg = format!(
                                "Đã nhập thành công {} profile từ {}",
                                imported_count,
                                path.display()
                            );
                            let js = format!(
                                "window.__onBackupResult(true, {});",
                                serde_json::to_string(&msg).unwrap_or_default()
                            );
                            let _ = webview.evaluate_script(&js);
                        }
                        Err(e) => {
                            let js = format!(
                                "window.__onBackupResult(false, {});",
                                serde_json::to_string(&e.to_string()).unwrap_or_default()
                            );
                            let _ = webview.evaluate_script(&js);
                            return;
                        }
                    }
                } else {
                    return;
                }
            }
            IpcMsg::DismissConflict => {
                self.conflict_modal = None;
            }
        }

        self.push_state(webview);
    }

    fn handle_session_event(&mut self, ev: SessionEvent, webview: &wry::WebView) {
        match ev {
            SessionEvent::StatusChanged { profile_id, status } => {
                if let Some(s) = self.sessions.get_mut(&profile_id) {
                    s.status = status.clone();
                    if matches!(
                        status,
                        ConnectionStatus::Disconnected | ConnectionStatus::Error(_)
                    ) {
                        for fw in s.forwards.values_mut() {
                            fw.status = ForwardStatus::Stopped;
                            fw.active_conns = 0;
                        }
                    }
                }
            }
            SessionEvent::ForwardStateUpdated {
                profile_id,
                rule_id,
                status,
                active_conns,
                total_conns,
                bytes_tx,
                bytes_rx,
            } => {
                if let Some(s) = self.sessions.get_mut(&profile_id) {
                    if let Some(fw) = s.forwards.get_mut(&rule_id) {
                        fw.status = status;
                        fw.active_conns = active_conns;
                        fw.total_conns = total_conns;
                        fw.bytes_tx = bytes_tx;
                        fw.bytes_rx = bytes_rx;
                    }
                }
            }
        }
        self.push_state(webview);
    }

    fn push_state(&self, webview: &wry::WebView) {
        if let Ok(json) = serde_json::to_string(&self.build_ui_state()) {
            let js = format!("window.__onState({});", json);
            let _ = webview.evaluate_script(&js);
        }
    }
}

fn main() -> wry::Result<()> {
    tracing_subscriber::fmt::init();

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Multi-SSH Connect")
        .with_inner_size(tao::dpi::LogicalSize::new(1180.0, 760.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(880.0, 560.0))
        .build(&event_loop)
        .unwrap();

    let html = include_str!("../ui/index.html");
    let ipc_proxy = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(move |req| {
            let _ = ipc_proxy.send_event(AppEvent::Ipc(req.body().to_string()));
        })
        .build(&window)?;

    let mut state = BackendState::new(proxy);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                for s in state.sessions.values() {
                    let _ = s.cmd_tx.send(SessionCommand::Disconnect);
                }
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(AppEvent::Ipc(raw)) => {
                state.handle_ipc(&raw, &webview);
            }
            Event::UserEvent(AppEvent::Session(ev)) => {
                state.handle_session_event(ev, &webview);
            }
            Event::UserEvent(AppEvent::TestResult { is_draft, result }) => {
                if is_draft {
                    let (ok, msg) = match result {
                        Ok(ms) => (true, format!("Kết nối SSH thành công ({} ms)", ms)),
                        Err(e) => (false, e),
                    };
                    let js = format!(
                        "window.__onDraftTestResult({}, {});",
                        ok,
                        serde_json::to_string(&msg).unwrap_or_default()
                    );
                    let _ = webview.evaluate_script(&js);
                } else {
                    state.notice = Some(match result {
                        Ok(ms) => UiNotice {
                            is_error: false,
                            message: format!("Test Connection thành công ({} ms)", ms),
                        },
                        Err(e) => UiNotice {
                            is_error: true,
                            message: e,
                        },
                    });
                    state.push_state(&webview);
                }
            }
            Event::UserEvent(AppEvent::ToolsDownloaded(res)) => {
                state.notice = Some(match res {
                    Ok(msg) => UiNotice {
                        is_error: false,
                        message: msg,
                    },
                    Err(e) => UiNotice {
                        is_error: true,
                        message: e,
                    },
                });
                state.push_state(&webview);
            }
            _ => {}
        }
    });
}
