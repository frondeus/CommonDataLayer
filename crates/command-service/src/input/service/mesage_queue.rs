use crate::output::OutputPlugin;
use crate::{
    communication::MessageRouter,
    input::{Error, MessageQueueConfig},
};
use futures::{Stream, stream::select_all};
use futures::stream::StreamExt; 
use log::{error, trace};
use std::process;
use tokio::pin;
use utils::message_types::BorrowedInsertMessage;
use utils::messaging_system::consumer::CommonConsumer;
use utils::messaging_system::message::CommunicationMessage;
use utils::messaging_system::Result;
use utils::metrics::counter;
use utils::task_limiter::TaskLimiter;

pub struct MessageQueueInput<P: OutputPlugin> {
    consumer: Vec<CommonConsumer>,
    message_router: MessageRouter<P>,
    task_limiter: TaskLimiter,
}

impl<P: OutputPlugin> MessageQueueInput<P> {
    pub async fn new(config: MessageQueueConfig, message_router: MessageRouter<P>) -> Result<Self, Error> {

        let mut consumers = Vec::new();
        for queue_name in config.queue_names {
            let consumer =
                CommonConsumer::new_rabbit(&config.connection_string, &config.consumer_tag, &queue_name)
                    .await
                    .map_err(Error::ConsumerCreationFailed)?;
            consumers.push(consumer);
        }

        Ok(Self {
            consumer:consumers,
            message_router,
            task_limiter: TaskLimiter::new(config.task_limit),
        })
    }

    async fn handle_message(
        router: MessageRouter<P>,
        message: Result<Box<dyn CommunicationMessage>>,
    ) -> Result<(), Error> {
        counter!("cdl.command-service.input-request", 1);
        let message = message.map_err(Error::FailedReadingMessage)?;

        let generic_message = Self::build_message(message.as_ref())?;

        trace!("Received message {:?}", generic_message);

        router
            .handle_message(generic_message)
            .await
            .map_err(Error::CommunicationError)?;

        message.ack().await.map_err(Error::FailedToAcknowledge)?;

        Ok(())
    }

    fn build_message(
        message: &'_ dyn CommunicationMessage,
    ) -> Result<BorrowedInsertMessage<'_>, Error> {
        let json = message.payload().map_err(Error::MissingPayload)?;
        let event: BorrowedInsertMessage =
            serde_json::from_str(json).map_err(Error::PayloadDeserializationFailed)?;

        Ok(event)
    }

    pub async fn listen(self) -> Result<(), Error> {
        let mut streams = Vec::new();
        for consumer in self.consumer {
            let consumer = consumer.leak();
            let stream = consumer.consume().await;
            streams.push(stream);
        };
        let message_stream = select_all(streams.into_iter().map(Box::pin));
        
        pin!(message_stream);

        while let Some(message) = message_stream.next().await {
            let router = self.message_router.clone();

            self.task_limiter
                .run(async move || {
                    if let Err(err) = Self::handle_message(router, message).await {
                        error!("Failed to handle message: {}", err);
                        process::abort();
                    }
                })
                .await;
        }

        Ok(())
    }
}
