//! Agent 循环基准测试
//!
//! 测试 Agent 循环处理消息的性能

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
        content: "Hello, test!".to_string(),
        timestamp: Utc::now(),
        media: Vec::new(),
        metadata: serde_json::json!({}),
    }
}

/// 单条消息处理基准测试
fn benchmark_single_message(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("single_message_processing", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let bus = Arc::new(MessageBus::new(100));
                let msg = create_test_message();
                bus.publish_inbound(black_box(msg)).await.unwrap();
            }
        });
    });
}

/// 批量消息处理基准测试
fn benchmark_batch_messages(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("batch_messages");

    for count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            count,
            |b, &count| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(count * 2));
                        for _ in 0..count {
                            let msg = create_test_message();
                            bus.publish_inbound(msg).await.unwrap();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// 消息序列化基准测试
fn benchmark_message_serialization(c: &mut Criterion) {
    let msg = create_test_message();

    c.bench_function("message_serialization", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&msg).unwrap());
        });
    });

    c.bench_function("message_deserialization", |b| {
        let serialized = serde_json::to_string(&msg).unwrap();
        b.iter(|| {
            black_box(serde_json::from_str::<InboundMessage>(&serialized).unwrap());
        });
    });
}

/// ChatMessage 创建基准测试
fn benchmark_chat_message_creation(c: &mut Criterion) {
    c.bench_function("chat_message_user", |b| {
        b.iter(|| {
            black_box(ChatMessage::user("Test message"));
        });
    });

    c.bench_function("chat_message_system", |b| {
        b.iter(|| {
            black_box(ChatMessage::system("System prompt"));
        });
    });

    c.bench_function("chat_message_assistant", |b| {
        b.iter(|| {
            black_box(ChatMessage::assistant("Response"));
        });
    });
}

/// 消息总线吞吐量基准测试
fn benchmark_bus_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("bus_throughput");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, &size| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(size));
                        let msg = create_test_message();

                        let mut handles = vec![];
                        for _ in 0..size {
                            let bus_clone = bus.clone();
                            let msg_clone = msg.clone();
                            handles.push(tokio::spawn(async move {
                                bus_clone.publish_inbound(msg_clone).await
                            }));
                        }

                        for handle in handles {
                            handle.await.unwrap().unwrap();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_single_message,
    benchmark_batch_messages,
    benchmark_message_serialization,
    benchmark_chat_message_creation,
    benchmark_bus_throughput
);
criterion_main!(benches);
