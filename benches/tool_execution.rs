//! 工具执行基准测试
//!
//! 测试工具系统的性能

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use stormclaw_core::agent::tools::{ToolRegistry, ReadFileTool, ExecTool};
use stormclaw_core::testing::{create_test_workspace};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// 工具注册基准测试
fn benchmark_tool_registration(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("tool_registration", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let registry = ToolRegistry::new();
                let tool = Arc::new(ReadFileTool);
                registry.register(black_box(tool)).await.unwrap();
            }
        });
    });
}

/// 文件读取工具基准测试
fn benchmark_read_file_tool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("read_file_tool", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let (_workspace, temp_dir) = create_test_workspace().await.unwrap();

                // 创建测试文件
                let test_file = temp_dir.path().join("test.txt");
                tokio::fs::write(&test_file, "Test content").await.unwrap();

                let tool = ReadFileTool;
                let args = serde_json::json!({
                    "path": test_file.to_string_lossy().to_string()
                });

                let result = tool.execute(black_box(args)).await;
                assert!(result.is_ok());

                drop(temp_dir);
            }
        });
    });
}

/// 工具执行基准测试
fn benchmark_tool_execution(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("tool_execution");

    // 测试不同大小的文件
    for size in [1024, 10240, 102400].iter() {
        group.bench_with_input(
            BenchmarkId::new("file_size", size),
            size,
            |b, &size| {
                b.to_async(&rt).iter(|| {
                    async {
                        let (_workspace, temp_dir) = create_test_workspace().await.unwrap();

                        // 创建测试文件
                        let test_file = temp_dir.path().join("test.txt");
                        let content = "x".repeat(size);
                        tokio::fs::write(&test_file, content).await.unwrap();

                        let tool = ReadFileTool;
                        let args = serde_json::json!({
                            "path": test_file.to_string_lossy().to_string()
                        });

                        let result = tool.execute(black_box(args)).await;
                        assert!(result.is_ok());

                        drop(temp_dir);
                    }
                });
            },
        );
    }

    group.finish();
}

/// 工具参数解析基准测试
fn benchmark_tool_parsing(c: &mut Criterion) {
    let args = serde_json::json!({
        "path": "/tmp/test.txt",
        "offset": 0,
        "limit": 100
    });

    c.bench_function("tool_args_parse", |b| {
        b.iter(|| {
            black_box(serde_json::from_value::<serde_json::Value>(args.clone()).unwrap());
        });
    });

    c.bench_function("tool_args_stringify", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&args).unwrap());
        });
    });
}

/// 多工具注册基准测试
fn benchmark_multiple_tool_registration(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("multiple_tools");

    for tool_count in [1, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(tool_count),
            tool_count,
            |b, &count| {
                b.to_async(&rt).iter(|| {
                    async {
                        let registry = ToolRegistry::new();

                        // 注册多个工具
                        for i in 0..count {
                            let tool = Arc::new(ReadFileTool);
                            // 使用不同的名称
                            registry.register_with_name(
                                black_box(format!("read_file_{}", i)),
                                tool
                            ).await.unwrap();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// 工具查找基准测试
fn benchmark_tool_lookup(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("tool_lookup", |b| {
        b.to_async(&rt).iter(|| {
            async {
                let registry = ToolRegistry::new();
                let tool = Arc::new(ReadFileTool);
                registry.register(tool).await.unwrap();

                // 查找工具
                let result = registry.get(black_box("read_file")).await;
                assert!(result.is_some());
            }
        });
    });
}

criterion_group!(
    benches,
    benchmark_tool_registration,
    benchmark_read_file_tool,
    benchmark_tool_execution,
    benchmark_tool_parsing,
    benchmark_multiple_tool_registration,
    benchmark_tool_lookup
);
criterion_main!(benches);
