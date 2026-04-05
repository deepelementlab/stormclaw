//! 消息总线基准测试
//!
//! 测试消息总线的性能

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use stormclaw_core::MessageBus;
use stormclaw_core::InboundMessage;
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

/// 消息发布基准测试
fn benchmark_publish(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("bus_publish", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let bus = Arc::new(MessageBus::new(1000));
                let msg = stormclaw_core::create_test_message();
                bus.publish_inbound(black_box(msg)).await.unwrap();
            }
        });
    });
}

/// 订阅者数量对性能的影响
fn benchmark_subscriber_impact(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("subscriber_impact");

    for subscriber_count in [0, 1, 5, 10].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(subscriber_count),
            subscriber_count,
            |b, &count| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(1000));

                        // 创建订阅者
                        let mut handles = vec![];
                        for _ in 0..count {
                            let bus_clone = bus.clone();
                            handles.push(tokio::spawn(async move {
                                let mut rx = bus_clone.subscribe_inbound();
                                while rx.recv().await.is_ok() {
                                    // 消费消息
                                }
                            }));
                        }

                        // 发布消息
                        let msg = create_test_message();
                        for _ in 0..100 {
                            bus.publish_inbound(msg.clone()).await.unwrap();
                        }

                        // 等待一小段时间让消息传播
                        tokio::time::sleep(Duration::from_millis(10)).await;

                        // 取消订阅者
                        for handle in handles {
                            handle.abort();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// 队列容量影响测试
fn benchmark_queue_capacity(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("queue_capacity");

    for capacity in [10, 100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(capacity),
            capacity,
            |b, &capacity| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(capacity));
                        let msg = stormclaw_core::create_test_message();

                        // 填充队列到一半
                        for _ in 0..(capacity / 2) {
                            bus.publish_inbound(msg.clone()).await.unwrap();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// 消息接收基准测试
fn benchmark_receive(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("bus_receive", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let bus = Arc::new(MessageBus::new(1000));
                let mut rx = bus.subscribe_inbound();

                // 发布消息的句柄
                let bus_clone = bus.clone();
                tokio::spawn(async move {
                    let msg = stormclaw_core::create_test_message();
                    bus_clone.publish_inbound(msg).await.unwrap();
                });

                // 接收消息
                black_box(rx.recv().await.unwrap());
            }
        });
    });
}

/// 并发发布基准测试
fn benchmark_concurrent_publish(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_publish");

    for concurrent_tasks in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrent_tasks),
            concurrent_tasks,
            |b, &tasks| {
                b.to_async(&rt).iter(|| {
                    async {
                        let bus = Arc::new(MessageBus::new(10000));
                        let msg = stormclaw_core::create_test_message();

                        let mut handles = vec![];
                        for _ in 0..tasks {
                            let bus_clone = bus.clone();
                            let msg_clone = msg.clone();
                            handles.push(tokio::spawn(async move {
                                for _ in 0..100 {
                                    bus_clone.publish_inbound(msg_clone.clone()).await.unwrap();
                                }
                            }));
                        }

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

criterion_group!(
    benches,
    benchmark_publish,
    benchmark_subscriber_impact,
    benchmark_queue_capacity,
    benchmark_receive,
    benchmark_concurrent_publish
);
criterion_main!(benches);
