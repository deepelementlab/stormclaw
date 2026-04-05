//! 消息总线模块
//!
//! 提供异步消息队列，用于解耦渠道和 Agent

pub mod events;
pub mod queue;

pub use events::{InboundMessage, OutboundMessage};
pub use queue::MessageBus;
