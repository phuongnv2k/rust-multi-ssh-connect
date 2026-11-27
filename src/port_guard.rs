use crate::models::{PortForwardRule, SshProfile};
use std::collections::HashMap;
use std::net::TcpListener;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePortBinding {
    pub profile_id: Uuid,
    pub profile_name: String,
    pub env_badge: String,
    pub rule_id: Uuid,
    pub rule_label: String,
    pub bind_address: String,
    pub local_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    /// Another currently connected profile is actively listening on this port. (HARD BLOCK)
    ActiveSession {
        owner_profile_id: Uuid,
        owner_profile_name: String,
        owner_env_badge: String,
        owner_rule_label: String,
        owner_bind: String,
    },
    /// Two enabled rules inside the same profile target the same local port. (HARD BLOCK)
    DuplicateWithinProfile { other_rule_label: String },
    /// Another saved (inactive) profile also configures this local port. (WARNING)
    SavedProfileOverlap {
        other_profile_id: Uuid,
        other_profile_name: String,
        other_env_badge: String,
        other_rule_label: String,
    },
    /// The OS reports the local port is already bound by an external process. (HARD BLOCK)
    OsPortInUse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortConflict {
    pub rule_id: Uuid,
    pub rule_label: String,
    pub bind_address: String,
    pub local_port: u16,
    pub kind: ConflictKind,
}

impl PortConflict {
    pub fn is_blocking(&self) -> bool {
        !matches!(self.kind, ConflictKind::SavedProfileOverlap { .. })
    }

    pub fn describe(&self) -> String {
        match &self.kind {
            ConflictKind::ActiveSession {
                owner_profile_name,
                owner_env_badge,
                owner_rule_label,
                owner_bind,
                ..
            } => format!(
                "Port {}:{} ({}) đang bị chiếm bởi session đang chạy [{}] {} ({} @ {})",
                self.bind_address,
                self.local_port,
                self.rule_label,
                owner_env_badge,
                owner_profile_name,
                owner_rule_label,
                owner_bind
            ),
            ConflictKind::DuplicateWithinProfile { other_rule_label } => format!(
                "Port {}:{} ({}) bị trùng với rule '{}' trong cùng profile",
                self.bind_address, self.local_port, self.rule_label, other_rule_label
            ),
            ConflictKind::SavedProfileOverlap {
                other_profile_name,
                other_env_badge,
                other_rule_label,
                ..
            } => format!(
                "Port {}:{} trùng cấu hình với [{}] {} ({})",
                self.bind_address,
                self.local_port,
                other_env_badge,
                other_profile_name,
                other_rule_label
            ),
            ConflictKind::OsPortInUse => format!(
                "Port {}:{} ({}) đang được sử dụng bởi một tiến trình khác trên hệ thống",
                self.bind_address, self.local_port, self.rule_label
            ),
        }
    }
}

/// Returns true if two bind addresses on the same port would collide.
/// `0.0.0.0` or `::` or `*` collides with any bind address.
pub fn binds_conflict(bind_a: &str, bind_b: &str) -> bool {
    let a = normalize_bind(bind_a);
    let b = normalize_bind(bind_b);
    if a == "0.0.0.0" || b == "0.0.0.0" || a == "::" || b == "::" || a == "*" || b == "*" {
        return true;
    }
    a.eq_ignore_ascii_case(&b)
}

fn normalize_bind(bind: &str) -> String {
    let t = bind.trim();
    if t.is_empty() {
        "127.0.0.1".to_string()
    } else if t.eq_ignore_ascii_case("localhost") {
        "127.0.0.1".to_string()
    } else {
        t.to_string()
    }
}

/// Checks if a single rule can bind at the OS level right now.
pub fn is_os_port_available(bind_address: &str, local_port: u16) -> bool {
    let bind = normalize_bind(bind_address);
    TcpListener::bind((bind.as_str(), local_port)).is_ok()
}

/// Checks all enabled port forwarding rules in `target` before connecting or enabling.
/// Returns all blocking conflicts (duplicate within profile, active session collision, or OS port in use).
pub fn check_activation_conflicts(
    target: &SshProfile,
    active_bindings: &[ActivePortBinding],
    check_os: bool,
) -> Vec<PortConflict> {
    let mut conflicts = Vec::new();
    let enabled_rules: Vec<&PortForwardRule> = target.enabled_port_forwards();

    for (idx, rule) in enabled_rules.iter().enumerate() {
        let bind = rule.normalized_bind();

        // 1. Check duplicates within the same profile
        for (other_idx, other) in enabled_rules.iter().enumerate() {
            if idx != other_idx
                && rule.local_port == other.local_port
                && binds_conflict(&bind, &other.normalized_bind())
            {
                conflicts.push(PortConflict {
                    rule_id: rule.id,
                    rule_label: rule.label.clone(),
                    bind_address: bind.clone(),
                    local_port: rule.local_port,
                    kind: ConflictKind::DuplicateWithinProfile {
                        other_rule_label: other.label.clone(),
                    },
                });
                break;
            }
        }

        // 2. Check against all currently active sessions (other profiles)
        for active in active_bindings {
            if active.profile_id != target.id
                && active.local_port == rule.local_port
                && binds_conflict(&bind, &active.bind_address)
            {
                conflicts.push(PortConflict {
                    rule_id: rule.id,
                    rule_label: rule.label.clone(),
                    bind_address: bind.clone(),
                    local_port: rule.local_port,
                    kind: ConflictKind::ActiveSession {
                        owner_profile_id: active.profile_id,
                        owner_profile_name: active.profile_name.clone(),
                        owner_env_badge: active.env_badge.clone(),
                        owner_rule_label: active.rule_label.clone(),
                        owner_bind: active.bind_address.clone(),
                    },
                });
            }
        }

        // 3. Check OS-level availability if no active session already flagged it
        if check_os {
            let already_flagged_active = conflicts.iter().any(|c| {
                c.rule_id == rule.id && matches!(c.kind, ConflictKind::ActiveSession { .. })
            });
            if !already_flagged_active && !is_os_port_available(&bind, rule.local_port) {
                conflicts.push(PortConflict {
                    rule_id: rule.id,
                    rule_label: rule.label.clone(),
                    bind_address: bind,
                    local_port: rule.local_port,
                    kind: ConflictKind::OsPortInUse,
                });
            }
        }
    }

    conflicts
}

/// Checks if a single rule would conflict when toggled ON in an already-connected session
/// or in the profile editor.
pub fn check_single_rule_conflict(
    owner_profile_id: Uuid,
    rule: &PortForwardRule,
    same_profile_rules: &[PortForwardRule],
    active_bindings: &[ActivePortBinding],
    check_os: bool,
) -> Option<PortConflict> {
    let bind = rule.normalized_bind();

    for other in same_profile_rules {
        if other.id != rule.id
            && other.enabled
            && other.local_port == rule.local_port
            && binds_conflict(&bind, &other.normalized_bind())
        {
            return Some(PortConflict {
                rule_id: rule.id,
                rule_label: rule.label.clone(),
                bind_address: bind,
                local_port: rule.local_port,
                kind: ConflictKind::DuplicateWithinProfile {
                    other_rule_label: other.label.clone(),
                },
            });
        }
    }

    for active in active_bindings {
        if active.profile_id != owner_profile_id
            && active.local_port == rule.local_port
            && binds_conflict(&bind, &active.bind_address)
        {
            return Some(PortConflict {
                rule_id: rule.id,
                rule_label: rule.label.clone(),
                bind_address: bind,
                local_port: rule.local_port,
                kind: ConflictKind::ActiveSession {
                    owner_profile_id: active.profile_id,
                    owner_profile_name: active.profile_name.clone(),
                    owner_env_badge: active.env_badge.clone(),
                    owner_rule_label: active.rule_label.clone(),
                    owner_bind: active.bind_address.clone(),
                },
            });
        }
    }

    if check_os && !is_os_port_available(&bind, rule.local_port) {
        return Some(PortConflict {
            rule_id: rule.id,
            rule_label: rule.label.clone(),
            bind_address: bind,
            local_port: rule.local_port,
            kind: ConflictKind::OsPortInUse,
        });
    }

    None
}

/// Computes static overlaps between all saved profiles so the UI can display early warnings.
pub fn check_saved_overlaps(profiles: &[SshProfile]) -> HashMap<Uuid, Vec<PortConflict>> {
    let mut map: HashMap<Uuid, Vec<PortConflict>> = HashMap::new();

    for (i, p1) in profiles.iter().enumerate() {
        for r1 in p1.enabled_port_forwards() {
            let b1 = r1.normalized_bind();
            for (j, p2) in profiles.iter().enumerate() {
                if i == j {
                    continue;
                }
                for r2 in p2.enabled_port_forwards() {
                    if r1.local_port == r2.local_port && binds_conflict(&b1, &r2.normalized_bind()) {
                        map.entry(p1.id).or_default().push(PortConflict {
                            rule_id: r1.id,
                            rule_label: r1.label.clone(),
                            bind_address: b1.clone(),
                            local_port: r1.local_port,
                            kind: ConflictKind::SavedProfileOverlap {
                                other_profile_id: p2.id,
                                other_profile_name: p2.name.clone(),
                                other_env_badge: p2.env_badge.clone(),
                                other_rule_label: r2.label.clone(),
                            },
                        });
                    }
                }
            }
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PortForwardRule, SshProfile};

    #[test]
    fn test_binds_conflict_matrix() {
        assert!(binds_conflict("127.0.0.1", "127.0.0.1"));
        assert!(binds_conflict("127.0.0.1", "localhost"));
        assert!(binds_conflict("0.0.0.0", "127.0.0.1"));
        assert!(binds_conflict("127.0.0.1", "0.0.0.0"));
        assert!(binds_conflict("::", "127.0.0.1"));
        assert!(!binds_conflict("127.0.0.1", "127.0.0.2"));
    }

    #[test]
    fn test_blocks_active_session_port_conflict() {
        let p1 = SshProfile::new("Prod DB", "10.0.0.1", "root");
        let r1 = PortForwardRule::new("MySQL", "127.0.0.1", 33060, "127.0.0.1", 3306);

        let active = vec![ActivePortBinding {
            profile_id: p1.id,
            profile_name: p1.name.clone(),
            env_badge: "PROD".to_string(),
            rule_id: r1.id,
            rule_label: r1.label.clone(),
            bind_address: "127.0.0.1".to_string(),
            local_port: 33060,
        }];

        let mut p2 = SshProfile::new("Staging DB", "10.0.0.2", "root");
        p2.port_forwards.push(PortForwardRule::new(
            "MySQL Stg",
            "0.0.0.0",
            33060,
            "127.0.0.1",
            3306,
        ));

        let conflicts = check_activation_conflicts(&p2, &active, false);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].is_blocking());
        assert!(matches!(conflicts[0].kind, ConflictKind::ActiveSession { .. }));
    }

    #[test]
    fn test_disabled_rule_does_not_conflict() {
        let p1 = SshProfile::new("Prod DB", "10.0.0.1", "root");
        let active = vec![ActivePortBinding {
            profile_id: p1.id,
            profile_name: p1.name.clone(),
            env_badge: "PROD".to_string(),
            rule_id: Uuid::new_v4(),
            rule_label: "MySQL".to_string(),
            bind_address: "127.0.0.1".to_string(),
            local_port: 33061,
        }];

        let mut p2 = SshProfile::new("Staging DB", "10.0.0.2", "root");
        let mut r2 = PortForwardRule::new("MySQL Stg", "127.0.0.1", 33061, "127.0.0.1", 3306);
        r2.enabled = false;
        p2.port_forwards.push(r2);

        let conflicts = check_activation_conflicts(&p2, &active, false);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_duplicate_within_same_profile_blocked() {
        let mut p = SshProfile::new("Multi", "10.0.0.1", "root");
        p.port_forwards.push(PortForwardRule::new(
            "Rule A",
            "127.0.0.1",
            8080,
            "127.0.0.1",
            80,
        ));
        p.port_forwards.push(PortForwardRule::new(
            "Rule B",
            "127.0.0.1",
            8080,
            "127.0.0.1",
            81,
        ));

        let conflicts = check_activation_conflicts(&p, &[], false);
        assert_eq!(conflicts.len(), 2);
        assert!(conflicts
            .iter()
            .all(|c| matches!(c.kind, ConflictKind::DuplicateWithinProfile { .. })));
    }

    #[test]
    fn test_saved_profile_overlap_warning() {
        let mut p1 = SshProfile::new("Server A", "10.0.0.1", "root");
        p1.port_forwards
            .push(PortForwardRule::new("Redis", "127.0.0.1", 6379, "127.0.0.1", 6379));

        let mut p2 = SshProfile::new("Server B", "10.0.0.2", "root");
        p2.port_forwards
            .push(PortForwardRule::new("Redis B", "127.0.0.1", 6379, "127.0.0.1", 6379));

        let overlaps = check_saved_overlaps(&[p1.clone(), p2.clone()]);
        assert_eq!(overlaps.get(&p1.id).map(|v| v.len()).unwrap_or(0), 1);
        assert_eq!(overlaps.get(&p2.id).map(|v| v.len()).unwrap_or(0), 1);
        assert!(!overlaps[&p1.id][0].is_blocking());
    }

    #[test]
    fn test_live_os_port_in_use_detection() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().unwrap().port();

        let mut p = SshProfile::new("Live Test", "10.0.0.1", "root");
        p.port_forwards.push(PortForwardRule::new(
            "Ephemeral",
            "127.0.0.1",
            port,
            "127.0.0.1",
            80,
        ));

        let conflicts = check_activation_conflicts(&p, &[], true);
        assert_eq!(conflicts.len(), 1);
        assert!(matches!(conflicts[0].kind, ConflictKind::OsPortInUse));

        drop(listener);
        let conflicts_after = check_activation_conflicts(&p, &[], true);
        assert!(conflicts_after.is_empty());
    }

    #[test]
    fn test_runtime_single_rule_toggle_conflict() {
        let p1_id = Uuid::new_v4();
        let p2_id = Uuid::new_v4();
        let active = vec![ActivePortBinding {
            profile_id: p1_id,
            profile_name: "Prod".to_string(),
            env_badge: "PROD".to_string(),
            rule_id: Uuid::new_v4(),
            rule_label: "PG".to_string(),
            bind_address: "127.0.0.1".to_string(),
            local_port: 54320,
        }];
        let candidate = PortForwardRule::new("PG2", "127.0.0.1", 54320, "127.0.0.1", 5432);
        let conflict = check_single_rule_conflict(p2_id, &candidate, &[], &active, false);
        assert!(conflict.is_some());
        assert!(conflict.unwrap().is_blocking());
    }
}
