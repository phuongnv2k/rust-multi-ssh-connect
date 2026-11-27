use crate::models::{AuthMethod, PortForwardRule, SshProfile};
use async_trait::async_trait;
use russh::client;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub type EventCallback = Arc<dyn Fn(SessionEvent) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardStatus {
    Starting,
    Listening,
    Stopped,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ForwardRuntimeState {
    pub rule: PortForwardRule,
    pub status: ForwardStatus,
    pub active_conns: u64,
    pub total_conns: u64,
    pub bytes_tx: u64,
    pub bytes_rx: u64,
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    StatusChanged {
        profile_id: Uuid,
        status: ConnectionStatus,
    },
    ForwardStateUpdated {
        profile_id: Uuid,
        rule_id: Uuid,
        status: ForwardStatus,
        active_conns: u64,
        total_conns: u64,
        bytes_tx: u64,
        bytes_rx: u64,
    },
}

#[derive(Debug)]
pub enum SessionCommand {
    StartForward(PortForwardRule),
    StopForward(Uuid),
    Disconnect,
}

struct SshClientHandler;

#[async_trait]
impl client::Handler for SshClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub struct SshRuntime {
    rt: tokio::runtime::Runtime,
}

impl SshRuntime {
    pub fn new() -> anyhow::Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        Ok(Self { rt })
    }

    pub fn spawn_session(
        &self,
        profile: SshProfile,
        allowed_forwards: Vec<PortForwardRule>,
        on_event: EventCallback,
    ) -> mpsc::UnboundedSender<SessionCommand> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        self.rt.spawn(async move {
            run_session_task(profile, allowed_forwards, cmd_rx, on_event).await;
        });
        cmd_tx
    }

    pub fn test_connection(
        &self,
        profile: SshProfile,
        on_done: Arc<dyn Fn(Result<u128, String>) + Send + Sync>,
    ) {
        self.rt.spawn(async move {
            let res = run_test_connection(profile).await;
            on_done(res);
        });
    }
}

async fn run_test_connection(profile: SshProfile) -> Result<u128, String> {
    let start = std::time::Instant::now();
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(10)),
        ..Default::default()
    });
    let addr = format!("{}:{}", profile.host.trim(), profile.port);

    let mut handle = match tokio::time::timeout(
        std::time::Duration::from_secs(8),
        client::connect(config, addr.as_str(), SshClientHandler),
    )
    .await
    {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => return Err(format!("Không kết nối được {}: {}", addr, e)),
        Err(_) => return Err(format!("Quá thời gian kết nối tới {} (timeout 8s)", addr)),
    };

    let auth_res = match &profile.auth {
        AuthMethod::Password { password } => {
            handle
                .authenticate_password(profile.username.trim(), password)
                .await
        }
        AuthMethod::PrivateKey {
            key_path,
            passphrase,
        } => {
            let pass = passphrase.as_deref().filter(|s| !s.is_empty());
            let key_pair = russh_keys::load_secret_key(key_path, pass)
                .map_err(|e| format!("Không đọc được SSH key: {}", e))?;
            let key_with_alg =
                russh_keys::key::PrivateKeyWithHashAlg::new(Arc::new(key_pair), None)
                    .map_err(|e| format!("Lỗi SSH key: {}", e))?;
            handle
                .authenticate_publickey(profile.username.trim(), key_with_alg)
                .await
        }
    };

    let elapsed = start.elapsed().as_millis();
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "Test connection", "en")
        .await;

    match auth_res {
        Ok(true) => Ok(elapsed),
        Ok(false) => Err("Xác thực thất bại (sai mật khẩu hoặc SSH key).".to_string()),
        Err(e) => Err(format!("Lỗi xác thực: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc as std_mpsc;

    #[test]
    fn test_connection_reports_error_on_unreachable_port() {
        let rt = SshRuntime::new().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_port = listener.local_addr().unwrap().port();
        drop(listener);

        let mut p = SshProfile::new("Unreachable", "127.0.0.1", "root");
        p.port = closed_port;

        let (tx, rx) = std_mpsc::channel();
        rt.test_connection(
            p,
            Arc::new(move |res| {
                let _ = tx.send(res);
            }),
        );

        let result = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("test_connection callback");
        assert!(result.is_err());
    }
}

async fn run_session_task(
    profile: SshProfile,
    allowed_forwards: Vec<PortForwardRule>,
    mut cmd_rx: mpsc::UnboundedReceiver<SessionCommand>,
    on_event: EventCallback,
) {
    let profile_id = profile.id;
    on_event(SessionEvent::StatusChanged {
        profile_id,
        status: ConnectionStatus::Connecting,
    });

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
        keepalive_interval: Some(std::time::Duration::from_secs(20)),
        ..Default::default()
    });

    let addr = format!("{}:{}", profile.host.trim(), profile.port);
    let mut handle = match client::connect(config, addr.as_str(), SshClientHandler).await {
        Ok(h) => h,
        Err(e) => {
            on_event(SessionEvent::StatusChanged {
                profile_id,
                status: ConnectionStatus::Error(format!("Kết nối thất bại: {}", e)),
            });
            return;
        }
    };

    let auth_ok = match &profile.auth {
        AuthMethod::Password { password } => {
            handle
                .authenticate_password(profile.username.trim(), password)
                .await
        }
        AuthMethod::PrivateKey {
            key_path,
            passphrase,
        } => {
            let pass = passphrase.as_deref().filter(|s| !s.is_empty());
            match russh_keys::load_secret_key(key_path, pass) {
                Ok(key_pair) => {
                    let key_with_alg =
                        russh_keys::key::PrivateKeyWithHashAlg::new(Arc::new(key_pair), None)
                            .unwrap();
                    handle
                        .authenticate_publickey(profile.username.trim(), key_with_alg)
                        .await
                }
                Err(e) => {
                    on_event(SessionEvent::StatusChanged {
                        profile_id,
                        status: ConnectionStatus::Error(format!("Không đọc được SSH key: {}", e)),
                    });
                    return;
                }
            }
        }
    };

    match auth_ok {
        Ok(true) => {}
        Ok(false) => {
            on_event(SessionEvent::StatusChanged {
                profile_id,
                status: ConnectionStatus::Error(
                    "Xác thực thất bại (sai mật khẩu hoặc key).".to_string(),
                ),
            });
            return;
        }
        Err(e) => {
            on_event(SessionEvent::StatusChanged {
                profile_id,
                status: ConnectionStatus::Error(format!("Lỗi xác thực: {}", e)),
            });
            return;
        }
    }

    let handle = Arc::new(handle);

    on_event(SessionEvent::StatusChanged {
        profile_id,
        status: ConnectionStatus::Connected,
    });

    let mut forward_tasks: HashMap<Uuid, JoinHandle<()>> = HashMap::new();
    for rule in allowed_forwards {
        let task = spawn_port_forwarder(
            profile_id,
            rule.clone(),
            Arc::clone(&handle),
            Arc::clone(&on_event),
        );
        forward_tasks.insert(rule.id, task);
    }

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            SessionCommand::StartForward(rule) => {
                if let Some(old) = forward_tasks.remove(&rule.id) {
                    old.abort();
                }
                let task = spawn_port_forwarder(
                    profile_id,
                    rule.clone(),
                    Arc::clone(&handle),
                    Arc::clone(&on_event),
                );
                forward_tasks.insert(rule.id, task);
            }
            SessionCommand::StopForward(rule_id) => {
                if let Some(task) = forward_tasks.remove(&rule_id) {
                    task.abort();
                }
                on_event(SessionEvent::ForwardStateUpdated {
                    profile_id,
                    rule_id,
                    status: ForwardStatus::Stopped,
                    active_conns: 0,
                    total_conns: 0,
                    bytes_tx: 0,
                    bytes_rx: 0,
                });
            }
            SessionCommand::Disconnect => break,
        }
    }

    for (_, task) in forward_tasks {
        task.abort();
    }
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "User disconnected", "en")
        .await;

    on_event(SessionEvent::StatusChanged {
        profile_id,
        status: ConnectionStatus::Disconnected,
    });
}

fn spawn_port_forwarder(
    profile_id: Uuid,
    rule: PortForwardRule,
    handle: Arc<client::Handle<SshClientHandler>>,
    on_event: EventCallback,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let bind_addr = format!("{}:{}", rule.normalized_bind(), rule.local_port);
        let listener = match TcpListener::bind(&bind_addr).await {
            Ok(l) => l,
            Err(e) => {
                on_event(SessionEvent::ForwardStateUpdated {
                    profile_id,
                    rule_id: rule.id,
                    status: ForwardStatus::Error(format!("Bind {} thất bại: {}", bind_addr, e)),
                    active_conns: 0,
                    total_conns: 0,
                    bytes_tx: 0,
                    bytes_rx: 0,
                });
                return;
            }
        };

        let active_conns = Arc::new(AtomicU64::new(0));
        let total_conns = Arc::new(AtomicU64::new(0));
        let bytes_tx = Arc::new(AtomicU64::new(0));
        let bytes_rx = Arc::new(AtomicU64::new(0));
        let alive = Arc::new(AtomicBool::new(true));

        on_event(SessionEvent::ForwardStateUpdated {
            profile_id,
            rule_id: rule.id,
            status: ForwardStatus::Listening,
            active_conns: 0,
            total_conns: 0,
            bytes_tx: 0,
            bytes_rx: 0,
        });

        loop {
            match listener.accept().await {
                Ok((mut tcp_stream, peer_addr)) => {
                    let h = Arc::clone(&handle);
                    let r_host = rule.remote_host.trim().to_string();
                    let r_port = rule.remote_port as u32;
                    let rule_id = rule.id;
                    let act = Arc::clone(&active_conns);
                    let tot = Arc::clone(&total_conns);
                    let btx = Arc::clone(&bytes_tx);
                    let brx = Arc::clone(&bytes_rx);
                    let cb = Arc::clone(&on_event);

                    act.fetch_add(1, Ordering::Relaxed);
                    tot.fetch_add(1, Ordering::Relaxed);
                    cb(SessionEvent::ForwardStateUpdated {
                        profile_id,
                        rule_id,
                        status: ForwardStatus::Listening,
                        active_conns: act.load(Ordering::Relaxed),
                        total_conns: tot.load(Ordering::Relaxed),
                        bytes_tx: btx.load(Ordering::Relaxed),
                        bytes_rx: brx.load(Ordering::Relaxed),
                    });

                    tokio::spawn(async move {
                        if let Ok(ssh_ch) = h
                            .channel_open_direct_tcpip(
                                r_host,
                                r_port,
                                peer_addr.ip().to_string(),
                                peer_addr.port() as u32,
                            )
                            .await
                        {
                            let mut ssh_stream = ssh_ch.into_stream();
                            if let Ok((up, down)) =
                                tokio::io::copy_bidirectional(&mut tcp_stream, &mut ssh_stream)
                                    .await
                            {
                                btx.fetch_add(up, Ordering::Relaxed);
                                brx.fetch_add(down, Ordering::Relaxed);
                            }
                        }
                        act.fetch_sub(1, Ordering::Relaxed);
                        cb(SessionEvent::ForwardStateUpdated {
                            profile_id,
                            rule_id,
                            status: ForwardStatus::Listening,
                            active_conns: act.load(Ordering::Relaxed),
                            total_conns: tot.load(Ordering::Relaxed),
                            bytes_tx: btx.load(Ordering::Relaxed),
                            bytes_rx: brx.load(Ordering::Relaxed),
                        });
                    });
                }
                Err(_) => {
                    if !alive.load(Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    })
}
