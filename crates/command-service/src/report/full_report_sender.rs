use crate::{
    communication::config::{CommunicationConfig, MessageQueueConfig},
    report::{Error, Reporter},
};
use log::{debug, trace};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use utils::message_types::OwnedInsertMessage;
use utils::messaging_system::publisher::CommonPublisher;
use uuid::Uuid;

const APPLICATION_NAME: &str = "Command Service";

#[derive(Clone)]
pub struct FullReportSenderBase {
    pub producer: CommonPublisher,
    pub topic: Arc<String>,
    pub output_plugin: Arc<String>,
}

pub struct FullReportSender {
    pub producer: CommonPublisher,
    pub topic: Arc<String>,
    pub output_plugin: Arc<String>,
    pub msg: OwnedInsertMessage,
}

#[derive(Serialize)]
struct ReportBody<'a> {
    application: &'static str,
    output_plugin: &'a str,
    description: &'a str,
    object_id: Uuid,
    payload: Value,
}

impl FullReportSenderBase {
    pub async fn new(
        communication_config: &CommunicationConfig,
        topic_or_exchange: String,
        output_plugin: String,
    ) -> Result<Self, Error> {
        let publisher = match communication_config {
            CommunicationConfig::MessageQueue(MessageQueueConfig::Kafka { brokers, .. }) => {
                CommonPublisher::new_kafka(brokers).await
            }
            CommunicationConfig::MessageQueue(MessageQueueConfig::Amqp {
                connection_string,
                ..
            }) => CommonPublisher::new_amqp(connection_string).await,
            CommunicationConfig::GRpc(_) => unreachable!(),
        };

        debug!(
            "Initialized report service with notification sink at `{}`",
            topic_or_exchange
        );

        Ok(Self {
            producer: publisher.map_err(Error::ProducerCreation)?,
            topic: Arc::new(topic_or_exchange),
            output_plugin: Arc::new(output_plugin),
        })
    }
}

#[async_trait::async_trait]
impl Reporter for FullReportSender {
    async fn report(self: Box<Self>, description: &str) -> Result<(), Error> {
        trace!("Report for id `{}` - `{}`", self.msg.object_id, description);

        let payload = ReportBody {
            application: APPLICATION_NAME,
            output_plugin: self.output_plugin.as_str(),
            description,
            object_id: self.msg.object_id,
            payload: self.msg.data,
        };

        self.producer
            .publish_message(
                self.topic.as_str(),
                "command_service.status",
                serde_json::to_vec(&payload).map_err(Error::FailedToProduceErrorMessage)?,
            )
            .await
            .map_err(Error::FailedToReport)
    }
}
