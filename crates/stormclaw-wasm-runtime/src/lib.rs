//! 受限 WASM 执行（MVP）：编译 WAT、计量 fuel、调用导出 `run` -> i32。

use anyhow::Context;
use wasmtime::{Config, Engine, Instance, Module, Store};

/// 运行资源上限
#[derive(Debug, Clone)]
pub struct WasmRunLimits {
    pub fuel: u64,
}

impl Default for WasmRunLimits {
    fn default() -> Self {
        Self { fuel: 50_000 }
    }
}

/// 解析 WAT、实例化无 import 模块，调用导出函数 `run`（无参，返回 i32）。
pub fn run_const_wasm_wat(wat: &str, limits: &WasmRunLimits) -> anyhow::Result<i32> {
    let wasm_bytes = wat::parse_str(wat).context("parse WAT")?;

    let mut cfg = Config::new();
    cfg.consume_fuel(true);
    let engine = Engine::new(&cfg).context("wasmtime engine")?;
    let module = Module::new(&engine, &wasm_bytes).context("wasm module")?;

    let mut store = Store::new(&engine, ());
    store.set_fuel(limits.fuel).context("set fuel")?;

    let instance = Instance::new(&mut store, &module, &[]).context("instance")?;
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .context("export `run` not found; guest must export (func (export \"run\") ...) -> i32")?;

    let out = run.call(&mut store, ()).context("wasm trap or fuel exhausted")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_minimal_wat() {
        let wat = r#"(module (func (export "run") (result i32) (i32.const 42)))"#;
        let v = run_const_wasm_wat(wat, &WasmRunLimits { fuel: 10_000 }).unwrap();
        assert_eq!(v, 42);
    }
}
