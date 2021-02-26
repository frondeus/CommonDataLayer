use anyhow::Context;
use async_trait::async_trait;
use lapin::message::Delivery;
use rdkafka::{message::BorrowedMessage, Message};

use super::Result;

#[async_trait]
pub trait CommunicationMessage: Send + Sync {
    fn payload(&self) -> Result<&str>;
    fn key(&self) -> Result<&str>;
}

pub struct KafkaCommunicationMessage<'a> {
    pub(super) message: BorrowedMessage<'a>,
}
#[async_trait]
impl<'a> CommunicationMessage for KafkaCommunicationMessage<'a> {
    fn key(&self) -> Result<&str> {
        let key = self
            .message
            .key()
            .ok_or_else(|| anyhow::anyhow!("Message has no key"))?;
        Ok(std::str::from_utf8(key)?)
    }
    fn payload(&self) -> Result<&str> {
        Ok(self
            .message
            .payload_view::<str>()
            .ok_or_else(|| anyhow::anyhow!("Message has no payload"))??)
    }
}

pub struct AmqpCommunicationMessage {
    pub(super) delivery: Delivery,
}
#[async_trait]
impl CommunicationMessage for AmqpCommunicationMessage {
    fn key(&self) -> Result<&str> {
        let key = self.delivery.routing_key.as_str();
        Ok(key)
    }
    fn payload(&self) -> Result<&str> {
        Ok(std::str::from_utf8(&self.delivery.data).context("Payload was not valid UTF-8")?)
    }
}
