//! Shell 命令执行：宿主或 Docker 隔离。

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

use stormclaw_config::DockerSecurityConfig;

use super::base::Tool;
use crate::agent::docker_exec::{docker_cli_available, run_command_in_docker};

/// Shell 命令执行工具
pub struct ExecTool {
    workspace: PathBuf,
    docker: DockerSecurityConfig,
}

impl ExecTool {
    pub fn new(workspace: PathBuf, docker: DockerSecurityConfig) -> Self {
        Self { workspace, docker }
    }

    async fn run_host(&self, command: &str) -> anyhow::Result<std::process::Output> {
        let working_dir = self.workspace.to_string_lossy().to_string();
        let output = if cfg!(windows) {
            tokio::process::Command::new("cmd")
                .args(["/C", command])
                .current_dir(&working_dir)
                .output()
                .await?
        } else {
            tokio::process::Command::new("sh")
                .args(["-c", command])
                .current_dir(&working_dir)
                .output()
                .await?
        };
        Ok(output)
    }

    fn format_output(output: std::process::Output) -> anyhow::Result<String> {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!("Command failed: {}\n{}", output.status, stderr);
        }

        if !stderr.is_empty() {
            Ok(format!("{}\n{}", stdout, stderr))
        } else {
            Ok(stdout)
        }
    }
}

#[async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the workspace directory (optionally inside Docker when configured)"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' argument"))?;

        tracing::debug!("Executing command (docker={}): {}", self.docker.enabled, command);

        if self.docker.enabled {
            if docker_cli_available().await {
                let out = run_command_in_docker(&self.workspace, command, &self.docker).await?;
                return Self::format_output(out);
            }
            if self.docker.fallback_to_host {
                tracing::warn!("Docker unavailable; falling back to host exec (fallback_to_host=true)");
                let out = self.run_host(command).await?;
                return Self::format_output(out);
            }
            anyhow::bail!(
                "exec is configured to use Docker but Docker is not available. \
                 Install/start Docker or set security.docker.fallbackToHost=true (not recommended)."
            );
        }

        let out = self.run_host(command).await?;
        Self::format_output(out)
    }
}
