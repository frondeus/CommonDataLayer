use crate::output::OutputPlugin;
use crate::{
    communication::{config::MessageQueueConfig, MessageRouter},
    input::Error,
};
use async_trait::async_trait;
use futures::future::try_join_all;
use log::{error, trace};
use std::{process, sync::Arc};
use utils::messaging_system::Result;
use utils::messaging_system::{consumer::CommonConsumer, get_order_group_id};
use utils::messaging_system::{consumer::ParConsumerHandler, message::CommunicationMessage};
use utils::metrics::counter;
use utils::task_limiter::TaskLimiter;
use utils::{message_types::BorrowedInsertMessage, parallel_task_queue::ParallelTaskQueue};

pub struct MessageQueueInput<P: OutputPlugin> {
    consumer: Vec<CommonConsumer>,
    handler: MessageQueueHandler<P>,
    task_limiter: TaskLimiter,
}

struct MessageQueueHandler<P: OutputPlugin> {
    message_router: MessageRouter<P>,
    task_queue: Arc<ParallelTaskQueue>,
}

impl<P: OutputPlugin> Clone for MessageQueueHandler<P> {
    fn clone(&self) -> Self {
        Self {
            message_router: self.message_router.clone(),
            task_queue: self.task_queue.clone(),
        }
    }
}

#[async_trait]
impl<P> ParConsumerHandler for MessageQueueHandler<P>
where
    P: OutputPlugin,
{
    async fn handle<'a>(&'a self, msg: &'a dyn CommunicationMessage) -> anyhow::Result<()> {
        let order_group_id = get_order_group_id(msg);
        let _guard = order_group_id
            .map(move |x| async move { self.task_queue.acquire_permit(x.to_string()).await });

        counter!("cdl.command-service.input-request", 1);

        let generic_message = Self::build_message(msg)?;

        trace!("Received message {:?}", generic_message);

        self.message_router
            .handle_message(generic_message)
            .await
            .map_err(Error::CommunicationError)?;

        Ok(())
    }
}

impl<P: OutputPlugin> MessageQueueHandler<P> {
    fn build_message(
        message: &'_ dyn CommunicationMessage,
    ) -> Result<BorrowedInsertMessage<'_>, Error> {
        let json = message.payload().map_err(Error::MissingPayload)?;
        let event: BorrowedInsertMessage =
            serde_json::from_str(json).map_err(Error::PayloadDeserializationFailed)?;

        Ok(event)
    }
}

impl<P: OutputPlugin> MessageQueueInput<P> {
    pub async fn new(
        config: MessageQueueConfig,
        message_router: MessageRouter<P>,
    ) -> Result<Self, Error> {
        let mut consumers = Vec::new();
        for ordered in config.ordered_configs() {
            let consumer = CommonConsumer::new(ordered)
                .await
                .map_err(Error::ConsumerCreationFailed)?;
            consumers.push(consumer);
        }

        for unordered in config.unordered_configs() {
            let consumer = CommonConsumer::new(unordered)
                .await
                .map_err(Error::ConsumerCreationFailed)?;
            consumers.push(consumer);
        }

        Ok(Self {
            consumer: consumers,
            task_limiter: TaskLimiter::new(config.task_limit()),
            handler: MessageQueueHandler {
                message_router,
                task_queue: Arc::new(ParallelTaskQueue::default()),
            },
        })
    }

    pub async fn listen(self) -> Result<(), Error> {
        trace!("Number of consumers: {}", self.consumer.len());

        let mut futures = vec![];
        for consumer in self.consumer {
            futures.push(consumer.par_run(self.handler.clone(), self.task_limiter.clone()));
        }

        if let Err(err) = try_join_all(futures).await {
            error!("Failed to handle message: {}", err);
            process::abort();
        }

        trace!("Stream closed");

        tokio::time::delay_for(tokio::time::Duration::from_secs(3)).await;

        Ok(())
    }
}
