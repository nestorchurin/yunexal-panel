use serde::Deserialize;
use std::collections::HashMap;
use bollard::models::{ContainerCreateBody, HostConfig, PortBinding, RestartPolicy, RestartPolicyNameEnum};

#[derive(Debug, Deserialize, Clone)]
pub struct ComposeService {
    pub image: Option<String>,
    #[allow(dead_code)]
    pub container_name: Option<String>,
    pub ports: Option<Vec<String>>,
    pub environment: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub restart: Option<String>,

    // Resource Limits
    pub cpus: Option<f64>,
    pub mem_limit: Option<String>,
    pub disk_limit: Option<String>,
}

impl ComposeService {
    pub fn to_container_config(&self, image_override: Option<String>) -> ContainerCreateBody {
        let image = image_override.or(self.image.clone());
        let env = self.environment.clone();

        // Parse ports → HostConfig.PortBindings
        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();

        if let Some(ports) = &self.ports {
            for port_str in ports {
                let (host_spec, container_spec_raw) = match port_str.split_once(':') {
                    Some((h, c)) => (h, c),
                    None => (port_str.as_str(), port_str.as_str()),
                };

                let (container_spec, proto_raw) = match container_spec_raw.split_once('/') {
                    Some((c, p)) => (c, Some(p)),
                    None => (container_spec_raw, None),
                };

                let protocols: Vec<&str> = match proto_raw {
                    Some("tcp+udp") => vec!["tcp", "udp"],
                    Some(p) => vec![p],
                    None => vec!["tcp"],
                };

                for proto in protocols {
                    let key = format!("{}/{}", container_spec, proto);
                    port_bindings.insert(
                        key,
                        Some(vec![PortBinding {
                            host_ip: Some("0.0.0.0".to_string()),
                            host_port: Some(host_spec.to_string()),
                        }]),
                    );
                }
            }
        }

        // Resources
        let nano_cpus = self.cpus.map(|c| (c * 1_000_000_000.0) as i64);
        let memory = self.mem_limit.as_ref().map(|s| parse_bytes(s));

        let storage_opt = self.disk_limit.as_ref().map(|s| {
            let mut opts = HashMap::new();
            opts.insert("size".to_string(), s.clone());
            opts
        });

        let host_config = HostConfig {
            port_bindings: if port_bindings.is_empty() { None } else { Some(port_bindings) },
            binds: self.volumes.clone(),
            restart_policy: self.restart.as_ref().map(|r| RestartPolicy {
                name: Some(match r.as_str() {
                    "always" => RestartPolicyNameEnum::ALWAYS,
                    "unless-stopped" => RestartPolicyNameEnum::UNLESS_STOPPED,
                    "on-failure" => RestartPolicyNameEnum::ON_FAILURE,
                    _ => RestartPolicyNameEnum::NO,
                }),
                maximum_retry_count: None,
            }),
            nano_cpus,
            memory,
            storage_opt,
            ..Default::default()
        };

        ContainerCreateBody {
            image,
            host_config: Some(host_config),
            env,
            tty: Some(true),
            open_stdin: Some(true),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        }
    }
}

fn parse_bytes(s: &str) -> i64 {
    let s = s.trim().to_lowercase();
    let digits_end = s.find(|c: char| !c.is_numeric() && c != '.').unwrap_or(s.len());
    let (num_str, unit) = s.split_at(digits_end);
    let num: f64 = num_str.parse().unwrap_or(0.0);

    match unit.trim() {
        "gb" | "g" => (num * 1024.0 * 1024.0 * 1024.0) as i64,
        "mb" | "m" => (num * 1024.0 * 1024.0) as i64,
        "kb" | "k" => (num * 1024.0) as i64,
        _ => num as i64,
    }
}
