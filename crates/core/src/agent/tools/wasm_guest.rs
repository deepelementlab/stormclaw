//! 可选 WASM 演示工具（需 `wasm-tools` 特性）

use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;

use stormclaw_wasm_runtime::{run_const_wasm_wat, WasmRunLimits};

use super::base::Tool;

/// 在沙箱内执行一段 WAT（无宿主 import），用于验证 wasmtime 管线。
pub struct WasmEvalTool;

#[async_trait]
impl Tool for WasmEvalTool {
    fn name(&self) -> &str {
        "wasm_eval_demo"
    }

    fn description(&self) -> &str {
        "Evaluate a tiny WASM module from WAT text; exports `run () -> i32`. For sandbox pipeline testing only."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "wat": {
                    "type": "string",
                    "description": "WebAssembly text; default returns 42"
                },
                "fuel": {
                    "type": "integer",
                    "description": "Wasmtime fuel budget"
                }
            }
        })
    }

    fn execution_timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(30))
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let wat = args["wat"].as_str().unwrap_or(
            r#"(module (func (export "run") (result i32) (i32.const 42)))"#,
        );
        let fuel = args["fuel"].as_u64().unwrap_or(50_000);
        let v = run_const_wasm_wat(wat, &WasmRunLimits { fuel })?;
        Ok(format!("wasm_eval_demo: i32 result = {}", v))
    }
}
