//! The execution backend details recorded for a run.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use url::Url;
use utoipa::ToSchema;
use wdl::engine::config::BackendConfig;

/// The execution backend a run executed on.
///
/// The backend information is built from an allowlist of identifying,
/// non-secret configuration fields rather than by serializing the backend
/// configuration, so secrets such as TES credentials are never persisted. New
/// backend configuration fields are excluded until they are explicitly added
/// here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RunBackend {
    /// The configured name the backend was selected by (e.g. `default` or a
    /// key under `[backends]`).
    name: String,
    /// The backend type (e.g. `docker`).
    #[serde(rename = "type")]
    backend_type: String,
    /// Identifying, non-secret configuration of the backend, which varies by
    /// backend type.
    #[schema(value_type = Object)]
    info: Value,
}

impl RunBackend {
    /// Creates the details for the backend selected by `name` with the given
    /// configuration.
    pub fn new(name: impl Into<String>, config: &BackendConfig) -> Self {
        let info = match config {
            BackendConfig::Local { config } => json!({
                "cpu": config.cpu,
                "memory": config.memory,
            }),
            BackendConfig::Docker { .. } => json!({}),
            BackendConfig::Tes { config } => json!({
                "service": sanitize_url(&config.service),
                "max_concurrency": config.max_concurrency,
            }),
            BackendConfig::LsfApptainer { config } => json!({
                "max_concurrency": config.max_concurrency,
                "default_lsf_queue": config.default_lsf_queue.as_ref().map(|q| &q.name),
                "short_task_lsf_queue": config.short_task_lsf_queue.as_ref().map(|q| &q.name),
                "gpu_lsf_queue": config.gpu_lsf_queue.as_ref().map(|q| &q.name),
                "fpga_lsf_queue": config.fpga_lsf_queue.as_ref().map(|q| &q.name),
            }),
            BackendConfig::SlurmApptainer { config } => json!({
                "max_concurrency": config.max_concurrency,
                "default_slurm_partition": config.default_slurm_partition.as_ref().map(|p| &p.name),
                "short_task_slurm_partition": config
                    .short_task_slurm_partition
                    .as_ref()
                    .map(|p| &p.name),
                "gpu_slurm_partition": config.gpu_slurm_partition.as_ref().map(|p| &p.name),
                "fpga_slurm_partition": config.fpga_slurm_partition.as_ref().map(|p| &p.name),
            }),
        };

        Self {
            name: name.into(),
            backend_type: config.name().to_string(),
            info,
        }
    }

    /// Gets the backend type (e.g. `docker`).
    pub fn backend_type(&self) -> &str {
        &self.backend_type
    }

    /// Gets the configured name the backend was selected by.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the identifying, non-secret configuration of the backend.
    pub fn info(&self) -> &Value {
        &self.info
    }

    /// Serializes the backend details to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("backend details should serialize")
    }
}

/// Removes the parts of a URL that may carry credentials: the user info,
/// query, and fragment.
fn sanitize_url(url: &Url) -> String {
    let mut url = url.clone();
    // These only fail for URLs that cannot have user info.
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.into()
}

#[cfg(test)]
mod tests {
    use wdl::engine::config::AdditionalArgs;
    use wdl::engine::config::ApptainerConfig;
    use wdl::engine::config::BasicAuthConfig;
    use wdl::engine::config::BearerAuthConfig;
    use wdl::engine::config::LocalBackendConfig;
    use wdl::engine::config::LsfApptainerBackendConfig;
    use wdl::engine::config::LsfQueueConfig;
    use wdl::engine::config::OAuthConfig;
    use wdl::engine::config::SlurmApptainerBackendConfig;
    use wdl::engine::config::SlurmPartitionConfig;
    use wdl::engine::config::TesBackendAuthConfig;
    use wdl::engine::config::TesBackendConfig;

    use super::*;

    /// Secret values placed in every field that may hold one.
    const SECRETS: &[&str] = &[
        "basic-password",
        "bearer-token",
        "oauth-client-secret",
        "service-user",
        "service-password",
        "service-query-token",
        "service-fragment",
        "inputs-sas-token",
        "outputs-sas-token",
        "extra-arg-secret",
    ];

    fn tes(auth: TesBackendAuthConfig) -> BackendConfig {
        BackendConfig::Tes {
            config: TesBackendConfig {
                cgroup_usage: true,
                resource_usage_metadata: false,
                service: "https://service-user:service-password@tes.example.com/ga4gh/tes?token=service-query-token#service-fragment"
                    .parse()
                    .unwrap(),
                auth: Some(auth),
                inputs: "s3://bucket/inputs/?sig=inputs-sas-token".parse().unwrap(),
                outputs: "s3://bucket/outputs/?sig=outputs-sas-token".parse().unwrap(),
                interval: Some(1),
                retries: Some(2),
                max_concurrency: Some(5),
                insecure: false,
            },
        }
    }

    fn assert_no_secrets(backend: &RunBackend) {
        let json = backend.to_json();
        for secret in SECRETS {
            assert!(
                !json.contains(secret),
                "backend `{json}` contains secret `{secret}`"
            );
        }
    }

    #[test]
    fn tes_info_excludes_secrets() {
        let auths = [
            TesBackendAuthConfig::Basic {
                config: BasicAuthConfig {
                    username: "user".into(),
                    password: Some("basic-password".into()),
                },
            },
            TesBackendAuthConfig::Bearer {
                config: BearerAuthConfig {
                    token: "bearer-token".into(),
                },
            },
            TesBackendAuthConfig::OAuth {
                config: Box::new(OAuthConfig {
                    client_id: "client".into(),
                    client_secret: Some("oauth-client-secret".into()),
                    audience: None,
                    authorization: "https://auth.example.com/device".parse().unwrap(),
                    token: "https://auth.example.com/token".parse().unwrap(),
                    scopes: Vec::new(),
                    require_refresh: false,
                }),
            },
        ];

        for auth in auths {
            let backend = RunBackend::new("cloud", &tes(auth));
            assert_eq!(
                backend.to_json(),
                r#"{"name":"cloud","type":"tes","info":{"service":"https://tes.example.com/ga4gh/tes","max_concurrency":5}}"#
            );
            assert_no_secrets(&backend);
        }
    }

    #[test]
    fn info_includes_only_allowlisted_fields() {
        let apptainer = ApptainerConfig {
            extra_args: vec!["--env=TOKEN=extra-arg-secret".into()],
            ..Default::default()
        };
        let args = AdditionalArgs {
            args: vec!["--extra-arg-secret".into()],
            ..Default::default()
        };
        let queue = |name: &str| LsfQueueConfig {
            name: name.into(),
            max_cpu_per_task: Some(4),
            max_memory_per_task: None,
        };
        let partition = |name: &str| SlurmPartitionConfig {
            name: name.into(),
            max_cpu_per_task: Some(4),
            max_memory_per_task: None,
        };

        let cases = [
            (
                BackendConfig::Local {
                    config: LocalBackendConfig {
                        cpu: Some(4),
                        memory: Some("8 GiB".into()),
                    },
                },
                "local",
                r#"{"cpu":4,"memory":"8 GiB"}"#,
            ),
            (
                BackendConfig::Docker {
                    config: Default::default(),
                },
                "docker",
                "{}",
            ),
            (
                BackendConfig::LsfApptainer {
                    config: LsfApptainerBackendConfig {
                        max_concurrency: Some(10),
                        default_lsf_queue: Some(queue("normal")),
                        short_task_lsf_queue: Some(queue("short")),
                        gpu_lsf_queue: Some(queue("gpu")),
                        fpga_lsf_queue: None,
                        job_name_prefix: Some("prefix".into()),
                        bsub: args.clone(),
                        apptainer: apptainer.clone(),
                        ..Default::default()
                    },
                },
                "lsf_apptainer",
                r#"{"max_concurrency":10,"default_lsf_queue":"normal","short_task_lsf_queue":"short","gpu_lsf_queue":"gpu","fpga_lsf_queue":null}"#,
            ),
            (
                BackendConfig::SlurmApptainer {
                    config: SlurmApptainerBackendConfig {
                        max_concurrency: None,
                        default_slurm_partition: Some(partition("normal")),
                        short_task_slurm_partition: None,
                        gpu_slurm_partition: Some(partition("gpu")),
                        fpga_slurm_partition: None,
                        job_name_prefix: Some("prefix".into()),
                        sbatch: args,
                        apptainer,
                        ..Default::default()
                    },
                },
                "slurm_apptainer",
                r#"{"max_concurrency":null,"default_slurm_partition":"normal","short_task_slurm_partition":null,"gpu_slurm_partition":"gpu","fpga_slurm_partition":null}"#,
            ),
        ];

        for (config, backend_type, expected) in cases {
            let backend = RunBackend::new("default", &config);
            assert_eq!(backend.backend_type(), backend_type);
            assert_eq!(backend.info().to_string(), expected);
            assert_no_secrets(&backend);
        }
    }
}
