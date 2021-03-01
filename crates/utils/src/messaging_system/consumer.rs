use crate::task_limiter::TaskLimiter;
use anyhow::Context;
use async_trait::async_trait;
use futures_util::TryStreamExt;
pub use lapin::options::BasicConsumeOptions;
use lapin::types::FieldTable;
use rdkafka::{
    consumer::{DefaultConsumerContext, StreamConsumer},
    ClientConfig,
};
use rpc::generic as proto;
use rpc::generic::generic_rpc_server::GenericRpc;
use rpc::generic::generic_rpc_server::GenericRpcServer;
use std::{net::SocketAddrV4, sync::Arc};
use tokio_amqp::LapinTokioExt;

use super::{
    kafka_ack_queue::KafkaAckQueue, message::AmqpCommunicationMessage,
    message::CommunicationMessage, message::KafkaCommunicationMessage, Error, Result,
};

#[async_trait]
pub trait ConsumerHandler {
    async fn handle<'a>(&'a mut self, msg: &'a dyn CommunicationMessage) -> anyhow::Result<()>;
}

#[async_trait]
pub trait ParConsumerHandler: Send + Sync + 'static {
    async fn handle<'a>(&'a self, msg: &'a dyn CommunicationMessage) -> anyhow::Result<()>;
}

struct GenericRpcImpl<T> {
    handler: Arc<T>,
}

#[tonic::async_trait]
impl<T> GenericRpc for GenericRpcImpl<T>
where
    T: ParConsumerHandler,
{
    async fn handle(
        &self,
        request: tonic::Request<proto::Message>,
    ) -> Result<tonic::Response<proto::Empty>, tonic::Status> {
        let msg = request.into_inner();

        match self.handler.handle(&msg).await {
            Ok(_) => Ok(tonic::Response::new(proto::Empty {})),
            Err(err) => Err(tonic::Status::internal(err.to_string())),
        }
    }
}

pub enum CommonConsumerConfig<'a> {
    Kafka {
        brokers: &'a str,
        group_id: &'a str,
        topic: &'a str,
    },
    Amqp {
        connection_string: &'a str,
        consumer_tag: &'a str,
        queue_name: &'a str,
        options: Option<BasicConsumeOptions>,
    },
    Grpc {
        addr: SocketAddrV4,
    },
}
pub enum CommonConsumer {
    Kafka {
        consumer: StreamConsumer<DefaultConsumerContext>,
        ack_queue: KafkaAckQueue,
    },
    Amqp {
        consumer: lapin::Consumer,
    },
    Grpc {
        addr: SocketAddrV4,
    },
}
impl CommonConsumer {
    pub async fn new(config: CommonConsumerConfig<'_>) -> Result<Self> {
        match config {
            CommonConsumerConfig::Kafka {
                group_id,
                brokers,
                topic,
            } => Self::new_kafka(group_id, brokers, &[topic]).await,
            CommonConsumerConfig::Amqp {
                connection_string,
                consumer_tag,
                queue_name,
                options,
            } => Self::new_amqp(connection_string, consumer_tag, queue_name, options).await,
            CommonConsumerConfig::Grpc { addr } => Ok(Self::Grpc { addr }),
        }
    }

    async fn new_kafka(group_id: &str, brokers: &str, topics: &[&str]) -> Result<Self> {
        let consumer: StreamConsumer<DefaultConsumerContext> = ClientConfig::new()
            .set("group.id", &group_id)
            .set("bootstrap.servers", &brokers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", "earliest")
            .create()
            .context("Consumer creation failed")?;

        rdkafka::consumer::Consumer::subscribe(&consumer, topics)
            .context("Can't subscribe to specified topics")?;

        Ok(CommonConsumer::Kafka {
            consumer,
            ack_queue: Default::default(),
        })
    }

    async fn new_amqp(
        connection_string: &str,
        consumer_tag: &str,
        queue_name: &str,
        consume_options: Option<BasicConsumeOptions>,
    ) -> Result<Self> {
        let consume_options = consume_options.unwrap_or_default();
        let connection = lapin::Connection::connect(
            connection_string,
            lapin::ConnectionProperties::default().with_tokio(),
        )
        .await?;
        let channel = connection.create_channel().await?;
        let consumer = channel
            .basic_consume(
                queue_name,
                consumer_tag,
                consume_options,
                FieldTable::default(),
            )
            .await?;
        Ok(CommonConsumer::Amqp { consumer })
    }

    /// Process messages in order. Cannot be used with Grpc.
    /// # Error handling
    /// Program exits on first unhandled message. I may cause crash-loop.
    pub async fn run(self, mut handler: impl ConsumerHandler) -> Result<()> {
        match self {
            CommonConsumer::Kafka {
                consumer,
                ack_queue,
            } => {
                let mut message_stream = consumer.start();
                while let Some(message) = message_stream.try_next().await? {
                    ack_queue.add(&message);
                    let message = KafkaCommunicationMessage { message };
                    match handler.handle(&message).await {
                        Ok(_) => {
                            ack_queue.ack(&message.message, &consumer);
                        }
                        Err(e) => {
                            log::error!("Couldn't process message: {:?}", e);
                            std::process::abort();
                        }
                    }
                }
            }
            CommonConsumer::Amqp { mut consumer } => {
                while let Some((channel, delivery)) = consumer.try_next().await? {
                    let message = AmqpCommunicationMessage { delivery };
                    match handler.handle(&message).await {
                        Ok(_) => {
                            channel
                                .basic_ack(message.delivery.delivery_tag, Default::default())
                                .await?;
                        }
                        Err(e) => {
                            log::error!("Couldn't process message: {:?}", e);
                            std::process::abort();
                        }
                    }
                }
            }
            CommonConsumer::Grpc { .. } => {
                // Use par_run instead
                return Err(Error::GrpcNotSupported);
            }
        }
        Ok(())
    }

    /// Process messages in parallel
    /// # Memory safety
    /// This method leaks kafka consumer
    /// # Error handling
    /// ## Kafka & AMQP
    /// Program exits on first unhandled message. I may cause crash-loop.
    /// ## GRPC
    /// Program returns 500 code and tries to handle further messages.
    pub async fn par_run(
        self,
        handler: impl ParConsumerHandler,
        task_limiter: TaskLimiter,
    ) -> Result<()> {
        let handler = Arc::new(handler);
        match self {
            CommonConsumer::Kafka {
                consumer,
                ack_queue,
            } => {
                let consumer = Box::leak(Box::new(Arc::new(consumer)));
                let ack_queue = Arc::new(ack_queue);
                let mut message_stream = consumer.start();
                while let Some(message) = message_stream.try_next().await? {
                    ack_queue.add(&message);
                    let ack_queue = ack_queue.clone();
                    let handler = handler.clone();
                    let consumer = consumer.clone();
                    task_limiter
                        .run(move || async move {
                            let message = KafkaCommunicationMessage { message };

                            match handler.handle(&message).await {
                                Ok(_) => {
                                    ack_queue.ack(&message.message, consumer.as_ref());
                                }
                                Err(e) => {
                                    log::error!("Couldn't process message: {:?}", e);
                                    std::process::abort();
                                }
                            }
                        })
                        .await;
                }
            }
            CommonConsumer::Amqp { mut consumer } => {
                while let Some((channel, delivery)) = consumer.try_next().await? {
                    let handler = handler.clone();
                    task_limiter
                        .run(move || async move {
                            let message = AmqpCommunicationMessage { delivery };
                            match handler.handle(&message).await {
                                Ok(_) => {
                                    if let Err(e) = channel
                                        .basic_ack(
                                            message.delivery.delivery_tag,
                                            Default::default(),
                                        )
                                        .await
                                    {
                                        log::error!("Couldn't ack message: {:?}", e);
                                        std::process::abort();
                                    }
                                }
                                Err(e) => {
                                    log::error!("Couldn't process message: {:?}", e);
                                    std::process::abort();
                                }
                            }
                        })
                        .await;
                }
            }
            CommonConsumer::Grpc { addr } => {
                tonic::transport::Server::builder()
                    .add_service(GenericRpcServer::new(GenericRpcImpl { handler }))
                    .serve(addr.into())
                    .await?;
            }
        }
        Ok(())
    }
}
