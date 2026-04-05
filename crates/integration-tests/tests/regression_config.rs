//! 配置序列化与默认值回归测试

use stormclaw_config::Config;

#[test]
fn config_default_roundtrip_json() {
    let cfg = Config::default();
    let json = serde_json::to_string(&cfg).expect("serialize");
    let back: Config = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.agents.defaults.model, cfg.agents.defaults.model);
}

#[test]
fn config_default_has_model_string() {
    let cfg = Config::default();
    assert!(!cfg.agents.defaults.model.is_empty());
}
