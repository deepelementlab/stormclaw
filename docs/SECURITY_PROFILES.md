# Security 配置模板（生产推荐）

本文提供两套可直接落地的 `security` 配置片段，均与当前 `stormclaw-config` 的 `SecurityConfig` 字段对齐（camelCase）。

使用方式：将以下 JSON 片段合并到你的 `config.json` 根对象中（即 `{"security": {...}}`）。

## Profile A：Baseline（推荐默认生产起步）

适用场景：希望尽快上线，同时保留较好兼容性与可观测性。

```json
{
  "security": {
    "ingressMode": "warn",
    "toolOutputSanitize": true,
    "injectionCheckOnToolOutput": true,
    "validatorStrict": false,
    "defaultToolTimeoutSecs": 90,
    "categoryTimeoutSecs": {
      "filesystem_read": 30,
      "filesystem_write": 45,
      "network": 60,
      "shell": 45,
      "meta": 30
    },
    "toolTimeoutSecs": {
      "web_fetch": 45,
      "web_search": 45,
      "exec": 30
    },
    "toolPolicies": {
      "web_search": {
        "enabled": true,
        "allowNetwork": true,
        "riskLevel": "high"
      },
      "web_fetch": {
        "enabled": true,
        "allowNetwork": true,
        "riskLevel": "high"
      },
      "exec": {
        "enabled": true,
        "requireDocker": true,
        "allowNetwork": false,
        "maxOutputBytes": 32768,
        "riskLevel": "high"
      }
    },
    "securityAudit": {
      "enabled": true,
      "sampleRate": 0.3,
      "includeRejectedReason": true
    },
    "docker": {
      "enabled": true,
      "image": "alpine:3.20",
      "workspaceWritable": false,
      "memoryMb": 512,
      "cpus": "1.0",
      "pidsLimit": 128,
      "tmpfsSizeMb": 64,
      "noNewPrivileges": true,
      "networkIsolated": true,
      "fallbackToHost": false
    },
    "wasmToolsEnabled": false
  }
}
```

## Profile B：Strict（高敏环境）

适用场景：生产高敏数据、面向不可信输入源、可接受更高误杀率。

```json
{
  "security": {
    "ingressMode": "enforce",
    "toolOutputSanitize": true,
    "injectionCheckOnToolOutput": true,
    "validatorStrict": true,
    "defaultToolTimeoutSecs": 45,
    "categoryTimeoutSecs": {
      "filesystem_read": 20,
      "filesystem_write": 30,
      "network": 25,
      "shell": 20,
      "meta": 20
    },
    "toolTimeoutSecs": {
      "exec": 15,
      "web_fetch": 20,
      "web_search": 20
    },
    "toolPolicies": {
      "exec": {
        "enabled": true,
        "requireDocker": true,
        "allowNetwork": false,
        "maxOutputBytes": 16384,
        "riskLevel": "high"
      },
      "web_search": {
        "enabled": false,
        "allowNetwork": false,
        "riskLevel": "high"
      },
      "web_fetch": {
        "enabled": false,
        "allowNetwork": false,
        "riskLevel": "high"
      }
    },
    "securityAudit": {
      "enabled": true,
      "sampleRate": 1.0,
      "includeRejectedReason": true
    },
    "docker": {
      "enabled": true,
      "image": "alpine:3.20",
      "workspaceWritable": false,
      "memoryMb": 256,
      "cpus": "0.5",
      "pidsLimit": 64,
      "tmpfsSizeMb": 32,
      "noNewPrivileges": true,
      "networkIsolated": true,
      "fallbackToHost": false
    },
    "wasmToolsEnabled": false
  }
}
```

## 切换建议

- 先用 Baseline 跑 3-7 天，关注 `stormclaw_security_audit` 的 `policy_blocked` / `validation_failed` 比例。
- 若误杀可控，再切 Strict；如果业务依赖联网检索，可只对 `exec` 严格，保留 `web_fetch/web_search`。
- 任何时候都建议保持：
  - `docker.enabled = true`
  - `docker.fallbackToHost = false`
  - `exec.requireDocker = true`

## 观测与告警建议

- 关注 `decision=policy_blocked` 比例：建议超过 5% 持续 10 分钟即告警（可能误杀或攻击流量）。
- 关注 `decision=timeout`：建议超过 2% 触发性能/网络排查。
- 关注 `security_tool_sanitized_bytes_total` 的突增：可能存在异常大输出或数据泄露风险。

