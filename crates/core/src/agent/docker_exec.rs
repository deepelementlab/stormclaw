//! 通过 Docker CLI 在隔离容器中执行 shell 命令。

use std::path::Path;
use tokio::process::Command;

use stormclaw_config::DockerSecurityConfig;

/// 本机 `docker` 是否可用（`docker info` 成功）。
pub async fn docker_cli_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 在工作区目录下执行 `sh -c <cmd>`；工作区挂载为 `/ws`。
pub async fn run_command_in_docker(
    workspace: &Path,
    cmd: &str,
    cfg: &DockerSecurityConfig,
) -> anyhow::Result<std::process::Output> {
    let ws = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    let ws_display = ws.to_string_lossy().to_string();
    let mount_mode = if cfg.workspace_writable { "rw" } else { "ro" };
    let mem = format!("{}m", cfg.memory_mb.max(32));

    let mut c = Command::new("docker");
    c.arg("run")
        .arg("--rm")
        .arg("-i")
        .args(["--user", "65534:65534"])
        .arg("--read-only")
        .args(["--cap-drop", "ALL"])
        .args(["--memory", &mem])
        .arg("-v")
        .arg(format!("{}:/ws:{}", ws_display, mount_mode))
        .args(["-w", "/ws"]);

    if cfg.network_isolated {
        c.args(["--network", "none"]);
    }

    c.arg(&cfg.image).args(["sh", "-c", cmd]);

    Ok(c.output().await?)
}
