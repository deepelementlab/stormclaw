//! 指标收集服务 (Metrics Service)
//!
//! 收集和暴露服务指标

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// 指标类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

/// 指标数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub labels: HashMap<String, String>,
    pub timestamp: i64,
}

/// 指标收集器
pub struct MetricsCollector {
    metrics: Arc<RwLock<HashMap<String, MetricData>>>,
}

#[derive(Debug, Clone)]
struct MetricData {
    metric_type: MetricType,
    value: Arc<AtomicU64>,
    fvalue: Arc<RwLock<f64>>,
    labels: Arc<RwLock<HashMap<String, String>>>,
    samples: Arc<RwLock<Vec<f64>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册计数器指标
    pub async fn register_counter(
        &self,
        name: String,
        help: String,
        labels: HashMap<String, String>,
    ) -> Counter {
        let mut metrics = self.metrics.write().await;
        let data = MetricData {
            metric_type: MetricType::Counter,
            value: Arc::new(AtomicU64::new(0)),
            fvalue: Arc::new(RwLock::new(0.0)),
            labels: Arc::new(RwLock::new(labels)),
            samples: Arc::new(RwLock::new(Vec::new())),
        };
        metrics.insert(name.clone(), data.clone());

        Counter {
            name,
            data: data.clone(),
            metrics: self.metrics.clone(),
        }
    }

    /// 注册仪表盘指标
    pub async fn register_gauge(
        &self,
        name: String,
        help: String,
        labels: HashMap<String, String>,
    ) -> Gauge {
        let mut metrics = self.metrics.write().await;
        let data = MetricData {
            metric_type: MetricType::Gauge,
            value: Arc::new(AtomicU64::new(0)),
            fvalue: Arc::new(RwLock::new(0.0)),
            labels: Arc::new(RwLock::new(labels)),
            samples: Arc::new(RwLock::new(Vec::new())),
        };
        metrics.insert(name.clone(), data.clone());

        Gauge {
            name,
            data: data.clone(),
            metrics: self.metrics.clone(),
        }
    }

    /// 注册直方图指标
    pub async fn register_histogram(
        &self,
        name: String,
        help: String,
        labels: HashMap<String, String>,
        buckets: Vec<f64>,
    ) -> Histogram {
        let mut metrics = self.metrics.write().await;
        let data = MetricData {
            metric_type: MetricType::Histogram,
            value: Arc::new(AtomicU64::new(0)),
            fvalue: Arc::new(RwLock::new(0.0)),
            labels: Arc::new(RwLock::new(labels)),
            samples: Arc::new(RwLock::new(buckets)),
        };
        metrics.insert(name.clone(), data.clone());

        Histogram {
            name,
            data: data.clone(),
            metrics: self.metrics.clone(),
        }
    }

    /// 获取所有指标（Prometheus 格式）
    pub async fn collect_prometheus(&self) -> String {
        let metrics = self.metrics.read().await;
        let mut lines = Vec::new();

        for (name, data) in metrics.iter() {
            let labels_str = data.labels.read().await;
            let labels_str = if labels_str.is_empty() {
                String::new()
            } else {
                let pairs: Vec<String> = labels_str.iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, v))
                    .collect();
                format!("{{{}}}", pairs.join(","))
            };

            lines.push(format!(
                "# {} {} {}",
                name,
                format!("{:?}", data.metric_type).to_lowercase(),
                name
            ));

            if data.samples.read().await.is_empty() {
                let value = if data.metric_type == MetricType::Counter {
                    data.value.load(Ordering::Relaxed) as f64
                } else {
                    *data.fvalue.read().await
                };

                lines.push(format!("{}{} {}", name, labels_str, value));
            } else {
                // 直方图数据
                let samples = data.samples.read().await;
                for (i, bucket) in samples.iter().enumerate() {
                    let le = format!("{}_le", bucket);
                    lines.push(format!("{}{} {} {}", name, labels_str, le, bucket));
                    lines.push(format!("{}{} {} {}", name, labels_str, le, i));
                }
            }
        }

        lines.join("\n")
    }

    /// 获取所有指标（JSON 格式）
    pub async fn collect_json(&self) -> Vec<Metric> {
        let metrics = self.metrics.read().await;
        let mut result = Vec::new();

        for (name, data) in metrics.iter() {
            let labels = data.labels.read().await.clone();

            result.push(Metric {
                name: name.clone(),
                metric_type: data.metric_type.clone(),
                value: if data.metric_type == MetricType::Counter {
                    data.value.load(Ordering::Relaxed) as f64
                } else {
                    *data.fvalue.read().await
                },
                labels,
                timestamp: chrono::Utc::now().timestamp_millis(),
            });
        }

        result
    }
}

/// 计数器指标
#[derive(Clone)]
pub struct Counter {
    name: String,
    data: MetricData,
    metrics: Arc<RwLock<HashMap<String, MetricData>>>,
}

impl Counter {
    /// 增加计数
    pub async fn inc(&self) {
        self.data.value.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加指定值
    pub async fn inc_by(&self, delta: u64) {
        self.data.value.fetch_add(delta, Ordering::Relaxed);
    }

    /// 获取当前值
    pub async fn get(&self) -> u64 {
        self.data.value.load(Ordering::Relaxed)
    }
}

/// 仪表盘指标
#[derive(Clone)]
pub struct Gauge {
    name: String,
    data: MetricData,
    metrics: Arc<RwLock<HashMap<String, MetricData>>>,
}

impl Gauge {
    /// 设置值
    pub async fn set(&self, value: f64) {
        let mut fvalue = self.data.fvalue.write().await;
        *fvalue = value;
    }

    /// 增加值
    pub async fn inc(&self) {
        let mut fvalue = self.data.fvalue.write().await;
        *fvalue += 1.0;
    }

    /// 减少值
    pub async fn dec(&self) {
        let mut fvalue = self.data.fvalue.write().await;
        *fvalue -= 1.0;
    }

    /// 获取当前值
    pub async fn get(&self) -> f64 {
        *self.data.fvalue.read().await
    }
}

/// 直方图指标
#[derive(Clone)]
pub struct Histogram {
    name: String,
    data: MetricData,
    metrics: Arc<RwLock<HashMap<String, MetricData>>>,
}

impl Histogram {
    /// 记录观测值
    pub async fn observe(&self, value: f64) {
        let mut samples = self.data.samples.write().await;
        samples.push(value);
    }

    /// 记录耗时
    pub async fn observe_duration<F, R>(&self, f: F) -> R
    where
        F: std::ops::FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_millis() as f64;
        self.observe(elapsed).await;
        result
    }
}

/// 服务指标
///
/// 预定义的服务指标
pub struct ServiceMetrics {
    messages_processed: Counter,
    messages_sent: Counter,
    messages_received: Counter,
    errors: Counter,
    active_connections: Gauge,
    request_duration: Histogram,
}

impl ServiceMetrics {
    pub async fn new(collector: &MetricsCollector) -> Self {
        let messages_processed = collector.register_counter(
            "stormclaw_messages_processed_total".to_string(),
            "Total number of messages processed".to_string(),
            HashMap::new(),
        ).await;

        let messages_sent = collector.register_counter(
            "stormclaw_messages_sent_total".to_string(),
            "Total number of messages sent".to_string(),
            HashMap::new(),
        ).await;

        let messages_received = collector.register_counter(
            "stormclaw_messages_received_total".to_string(),
            "Total number of messages received".to_string(),
            HashMap::new(),
        ).await;

        let errors = collector.register_counter(
            "stormclaw_errors_total".to_string(),
            "Total number of errors".to_string(),
            HashMap::new(),
        ).await;

        let active_connections = collector.register_gauge(
            "stormclaw_active_connections".to_string(),
            "Number of active connections".to_string(),
            HashMap::new(),
        ).await;

        let request_duration = collector.register_histogram(
            "stormclaw_request_duration_ms".to_string(),
            "Request duration in milliseconds".to_string(),
            HashMap::new(),
            vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0],
        ).await;

        Self {
            messages_processed,
            messages_sent,
            messages_received,
            errors,
            active_connections,
            request_duration,
        }
    }

    pub async fn record_message(&self) {
        self.messages_processed.inc().await;
    }

    pub async fn record_sent(&self) {
        self.messages_sent.inc().await;
    }

    pub async fn record_received(&self) {
        self.messages_received.inc().await;
    }

    pub async fn record_error(&self) {
        self.errors.inc().await;
    }

    pub async fn connection_opened(&self) {
        self.active_connections.inc().await;
    }

    pub async fn connection_closed(&self) {
        self.active_connections.dec().await;
    }
}
