//! 性能指标收集模块
//!
//! 提供性能数据的收集、聚合和报告功能

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// 性能指标收集器
#[derive(Clone)]
pub struct Metrics {
    /// 处理的消息总数
    messages_processed: Arc<AtomicU64>,
    /// 总处理时间（纳秒）
    total_processing_time_ns: Arc<AtomicU64>,
    /// 工具执行次数
    tool_executions: Arc<AtomicU64>,
    /// 工具执行总时间（纳秒）
    total_tool_time_ns: Arc<AtomicU64>,
    /// 错误计数
    errors: Arc<AtomicU64>,
    /// 最后更新时间
    last_update: Arc<tokio::sync::RwLock<Option<Instant>>>,
}

impl Metrics {
    /// 创建新的指标收集器
    pub fn new() -> Self {
        Self {
            messages_processed: Arc::new(AtomicU64::new(0)),
            total_processing_time_ns: Arc::new(AtomicU64::new(0)),
            tool_executions: Arc::new(AtomicU64::new(0)),
            total_tool_time_ns: Arc::new(AtomicU64::new(0)),
            errors: Arc::new(AtomicU64::new(0)),
            last_update: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// 记录消息处理
    pub fn record_message(&self, duration: Duration) {
        self.messages_processed.fetch_add(1, Ordering::Relaxed);
        self.total_processing_time_ns.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        *self.last_update.blocking_write() = Some(Instant::now());
    }

    /// 记录工具执行
    pub fn record_tool_execution(&self, duration: Duration) {
        self.tool_executions.fetch_add(1, Ordering::Relaxed);
        self.total_tool_time_ns.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        *self.last_update.blocking_write() = Some(Instant::now());
    }

    /// 记录错误
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        *self.last_update.blocking_write() = Some(Instant::now());
    }

    /// 获取平均消息处理时间
    pub fn average_processing_time(&self) -> Option<Duration> {
        let count = self.messages_processed.load(Ordering::Relaxed);
        if count == 0 {
            None
        } else {
            let total = self.total_processing_time_ns.load(Ordering::Relaxed);
            Some(Duration::from_nanos(total / count))
        }
    }

    /// 获取平均工具执行时间
    pub fn average_tool_time(&self) -> Option<Duration> {
        let count = self.tool_executions.load(Ordering::Relaxed);
        if count == 0 {
            None
        } else {
            let total = self.total_tool_time_ns.load(Ordering::Relaxed);
            Some(Duration::from_nanos(total / count))
        }
    }

    /// 获取吞吐量（每秒消息数）
    pub fn throughput(&self) -> Option<f64> {
        if let Some(last_update) = *self.last_update.try_read().ok()? {
            let elapsed = last_update.elapsed().as_secs_f64();
            let count = self.messages_processed.load(Ordering::Relaxed);
            if count > 0 && elapsed > 0.0 {
                Some(count as f64 / elapsed)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 获取错误率
    pub fn error_rate(&self) -> Option<f64> {
        let total = self.messages_processed.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);
        if total == 0 {
            None
        } else {
            Some((errors as f64 / total as f64) * 100.0)
        }
    }

    /// 获取指标摘要
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            messages_processed: self.messages_processed.load(Ordering::Relaxed),
            tool_executions: self.tool_executions.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            average_processing_time_ms: self.average_processing_time()
                .map(|d| d.as_secs_f64() * 1000.0),
            average_tool_time_ms: self.average_tool_time()
                .map(|d| d.as_secs_f64() * 1000.0),
            throughput_per_sec: self.throughput(),
            error_rate_percent: self.error_rate(),
        }
    }

    /// 重置所有指标
    pub fn reset(&self) {
        self.messages_processed.store(0, Ordering::Relaxed);
        self.total_processing_time_ns.store(0, Ordering::Relaxed);
        self.tool_executions.store(0, Ordering::Relaxed);
        self.total_tool_time_ns.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        *self.last_update.blocking_write() = None;
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 指标摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub messages_processed: u64,
    pub tool_executions: u64,
    pub errors: u64,
    pub average_processing_time_ms: Option<f64>,
    pub average_tool_time_ms: Option<f64>,
    pub throughput_per_sec: Option<f64>,
    pub error_rate_percent: Option<f64>,
}

/// 性能计时器
pub struct Timer {
    start: Instant,
    metrics: Option<Metrics>,
    timer_type: TimerType,
}

enum TimerType {
    Message,
    Tool,
}

impl Timer {
    /// 创建消息处理计时器
    pub fn message() -> Self {
        Self {
            start: Instant::now(),
            metrics: None,
            timer_type: TimerType::Message,
        }
    }

    /// 创建工具执行计时器
    pub fn tool() -> Self {
        Self {
            start: Instant::now(),
            metrics: None,
            timer_type: TimerType::Tool,
        }
    }

    /// 关联指标收集器
    pub fn with_metrics(mut self, metrics: Metrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// 停止计时并记录
    pub fn stop(mut self) -> Duration {
        let duration = self.start.elapsed();
        if let Some(metrics) = self.metrics.take() {
            match self.timer_type {
                TimerType::Message => metrics.record_message(duration),
                TimerType::Tool => metrics.record_tool_execution(duration),
            }
        }
        duration
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // 如果显式调用 stop，这里不会重复记录
        // 但如果忘记调用 stop，会自动记录
        let duration = self.start.elapsed();
        if let Some(metrics) = &self.metrics {
            match self.timer_type {
                TimerType::Message => metrics.record_message(duration),
                TimerType::Tool => metrics.record_tool_execution(duration),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        assert_eq!(metrics.messages_processed.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.tool_executions.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_message() {
        let metrics = Metrics::new();
        metrics.record_message(Duration::from_millis(100));
        metrics.record_message(Duration::from_millis(200));

        assert_eq!(metrics.messages_processed.load(Ordering::Relaxed), 2);
        let avg = metrics.average_processing_time().unwrap();
        assert_eq!(avg.as_millis(), 150);
    }

    #[test]
    fn test_record_tool() {
        let metrics = Metrics::new();
        metrics.record_tool_execution(Duration::from_millis(50));
        metrics.record_tool_execution(Duration::from_millis(150));

        assert_eq!(metrics.tool_executions.load(Ordering::Relaxed), 2);
        let avg = metrics.average_tool_time().unwrap();
        assert_eq!(avg.as_millis(), 100);
    }

    #[test]
    fn test_error_rate() {
        let metrics = Metrics::new();
        metrics.record_message(Duration::from_millis(100));
        metrics.record_message(Duration::from_millis(100));
        metrics.record_error();

        let error_rate = metrics.error_rate().unwrap();
        assert_eq!(error_rate, 50.0);
    }

    #[test]
    fn test_metrics_summary() {
        let metrics = Metrics::new();
        metrics.record_message(Duration::from_millis(100));
        metrics.record_tool_execution(Duration::from_millis(50));

        let summary = metrics.summary();
        assert_eq!(summary.messages_processed, 1);
        assert_eq!(summary.tool_executions, 1);
        assert_eq!(summary.average_processing_time_ms, Some(100.0));
        assert_eq!(summary.average_tool_time_ms, Some(50.0));
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = Metrics::new();
        metrics.record_message(Duration::from_millis(100));
        metrics.record_error();

        metrics.reset();

        assert_eq!(metrics.messages_processed.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_timer() {
        let metrics = Metrics::new();
        let duration = Timer::message()
            .with_metrics(metrics.clone())
            .stop();

        assert!(duration.as_millis() >= 0);
        assert_eq!(metrics.messages_processed.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_timer_drop() {
        let metrics = Metrics::new();
        {
            let _timer = Timer::tool().with_metrics(metrics.clone());
            thread::sleep(Duration::from_millis(10));
        } // timer 自动 drop

        assert_eq!(metrics.tool_executions.load(Ordering::Relaxed), 1);
    }
}
