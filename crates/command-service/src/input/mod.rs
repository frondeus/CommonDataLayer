pub use crate::input::config::{GRpcConfig, InputConfig, MessageQueueConfig};
pub use crate::input::error::Error;
pub use crate::input::service::{GRPCInput, MessageQueueInput};

mod config;
mod error;
mod service;
