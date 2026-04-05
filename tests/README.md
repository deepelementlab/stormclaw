# 旧版根目录 `tests/` 说明

集成测试与共享辅助代码已迁移至 workspace 成员：

- [`crates/integration-tests/`](../crates/integration-tests)（包名 `stormclaw-integration-tests`）

在 `stormclaw` 目录下执行：

```bash
cargo test -p stormclaw-integration-tests
# 或运行整个工作区（含单元测试与上述回归测试）
cargo test --workspace
```
