//! 消息总线基本收发回归测试

use std::sync::Arc;
use stormclaw_core::{InboundMessage, MessageBus, OutboundMessage};
use stormclaw_integration_tests::common::wait_for_inbound;

#[tokio::test]
async fn publish_inbound_consumed() {
    let bus = Arc::new(MessageBus::new(16));
    let waiter = bus.clone();
    let h = tokio::spawn(async move { wait_for_inbound(waiter, 2000).await });

    let msg = InboundMessage::new("telegram", "u1", "c1", "ping");
    bus.publish_inbound(msg.clone()).await.unwrap();

    let got = h.await.unwrap().unwrap();
    assert_eq!(got.content, "ping");
    assert_eq!(got.channel, "telegram");
}

#[tokio::test]
async fn publish_outbound_broadcast_received() {
    let bus = Arc::new(MessageBus::new(16));
    let ch = "telegram";
    let mut rx = bus.subscribe_outbound(ch.to_string()).await;

    let out = OutboundMessage::new(ch, "chat1", "pong");
    bus.publish_outbound(out).await.unwrap();

    let got = rx.recv().await.unwrap();
    assert_eq!(got.content, "pong");
}
