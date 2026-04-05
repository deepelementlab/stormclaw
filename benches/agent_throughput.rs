//! Agent 吞吐量基准测试
//!
//! 测试 Agent 在高负载下的性能表现

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use stormclaw_core::{MessageBus, InboundMessage, ChatMessage};
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;

fn create_test_message() -> InboundMessage {
    InboundMessage {
        channel: "test".to_string(),
        sender_id: "user123".to_string(),
        chat_id: "chat123".to_string(),
        content: "Test message for throughput".to_string(),
        timestamp: Utc::now(),
        media: Vec::new(),
        metadata: serde_json::json!({}),
    }
}

/// Agent 吞吐量测试
fn benchmark_agent_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("agent_throughput");

    for message_count in [10, 50, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::new("messages", message_count),
            message_count,
            |b, &count| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(count * 2));

                        // 批量发送消息
                        for _ in 0..count {
                            let msg = create_test_message();
                            bus.publish_inbound(black_box(msg)).await.unwrap();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// 并发消息处理测试
fn benchmark_concurrent_processing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_processing");

    for task_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("tasks", task_count),
            task_count,
            |b, &tasks| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(10000));

                        // 并发发送消息
                        let handles: Vec<_> = (0..tasks)
                            .map(|_| {
                                let bus_clone = bus.clone();
                                tokio::spawn(async move {
                                    for i in 0..100 {
                                        let msg = InboundMessage {
                                            channel: "test".to_string(),
                                            sender_id: format!("user_{}", i),
                                            chat_id: format!("chat_{}", i),
                                            content: "Test".to_string(),
                                            timestamp: Utc::now(),
                                            media: Vec::new(),
                                            metadata: serde_json::json!({}),
                                        };
                                        bus_clone.publish_inbound(msg).await.unwrap();
                                    }
                                })
                            })
                            .collect();

                        // 等待所有任务完成
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// 会话管理吞吐量测试
fn benchmark_session_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("session_create", |b| {
        b.to_async(&rt).iter(|| {
            async {
                use stormclaw_core::Session;
                let _session = black_box(Session::new("test_key"));
            }
        });
    });

    c.bench_function("session_add_message", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let mut session = stormclaw_core::Session::new("test_key");
                for i in 0..10 {
                    session.add_message("user", format!("Message {}", i));
                }
            }
        });
    });
}

/// 内存使用测试
fn benchmark_memory_usage(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_usage");

    for message_size in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("message_size", message_size),
            message_size,
            |b, &size| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(1000));
                        let content = "x".repeat(*size);

                        let msg = InboundMessage {
                            channel: "test".to_string(),
                            sender_id: "user123".to_string(),
                            chat_id: "chat123".to_string(),
                            content,
                            timestamp: Utc::now(),
                            media: Vec::new(),
                            metadata: serde_json::json!({}),
                        };

                        bus.publish_inbound(black_box(msg)).await.unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// 消息序列化性能
fn benchmark_serialization(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("serialize_inbound", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let msg = InboundMessage {
                    channel: "telegram".to_string(),
                    sender_id: "user123".to_string(),
                    chat_id: "chat123".to_string(),
                    content: "Hello, world!".to_string(),
                    timestamp: Utc::now(),
                    media: vec!["photo1.jpg".to_string()],
                    metadata: serde_json::json!({"key": "value"}),
                };
                black_box(serde_json::to_vec(&msg).unwrap());
            }
        });
    });

    c.bench_function("deserialize_inbound", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let msg = InboundMessage {
                    channel: "telegram".to_string(),
                    sender_id: "user123".to_string(),
                    chat_id: "chat123".to_string(),
                    content: "Hello, world!".to_string(),
                    timestamp: Utc::now(),
                    media: vec!["photo1.jpg".to_string()],
                    metadata: serde_json::json!({"key": "value"}),
                };
                let serialized = serde_json::to_vec(&msg).unwrap();
                black_box(serde_json::from_slice::<InboundMessage>(&serialized).unwrap());
            }
        });
    });
}

criterion_group!(
    benches,
    benchmark_agent_throughput,
    benchmark_concurrent_processing,
    benchmark_session_operations,
    benchmark_memory_usage,
    benchmark_serialization
);
criterion_main!(benches);
