//! AgentLoop 短路径回归：`process_direct` + MockLLMProvider（无外网）。

use std::sync::Arc;

use stormclaw_config::SecurityConfig;
use stormclaw_core::testing::MockLLMProvider;
use stormclaw_core::{AgentLoop, MessageBus};
use tempfile::tempdir;

#[tokio::test]
async fn agent_loop_process_direct_mock_provider() {
    let dir = tempdir().expect("tempdir");
    let bus = Arc::new(MessageBus::new(100));
    let mock = Arc::new(MockLLMProvider::new());
    mock.add_response("mock-reply-ok").await;

    let agent = AgentLoop::new(
        bus,
        mock.clone(),
        dir.path().to_path_buf(),
        Some("gpt-4".into()),
        4,
        None,
        SecurityConfig::default(),
    )
    .await
    .expect("AgentLoop::new");

    let out = agent
        .process_direct("ping", "cli:regression")
        .await
        .expect("process_direct");

    assert!(
        out.contains("mock-reply-ok"),
        "expected mock content in response, got: {out:?}"
    );
}
