use anyhow::Context;
use async_trait::async_trait;
use log::{debug, error, trace};
use lru_cache::LruCache;
use rpc::schema_registry::Id;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    process,
    sync::{Arc, Mutex},
};
use structopt::{clap::arg_enum, StructOpt};
use utils::{
    abort_on_poison,
    message_types::BorrowedInsertMessage,
    messaging_system::{
        consumer::{CommonConsumer, ParConsumerHandler},
        get_order_group_id,
        message::CommunicationMessage,
        publisher::CommonPublisher,
    },
    metrics::{self, counter},
    parallel_task_queue::ParallelTaskQueue,
    task_limiter::TaskLimiter,
};
use utils::{
    current_timestamp, message_types::DataRouterInsertMessage,
    messaging_system::consumer::CommonConsumerConfig,
};
use uuid::Uuid;

const SERVICE_NAME: &str = "data-router";

arg_enum! {
    #[derive(Deserialize, Debug, Serialize)]
    enum MessageQueueKind {
        Kafka,
        Amqp
    }
}

#[derive(StructOpt, Deserialize, Debug, Serialize)]
struct Config {
    #[structopt(long, env, possible_values = &MessageQueueKind::variants(), case_insensitive = true)]
    pub message_queue: MessageQueueKind,
    #[structopt(long, env)]
    pub kafka_brokers: Option<String>,
    #[structopt(long, env)]
    pub kafka_group_id: Option<String>,
    #[structopt(long, env)]
    pub amqp_connection_string: Option<String>,
    #[structopt(long, env)]
    pub amqp_consumer_tag: Option<String>,
    #[structopt(long, env)]
    pub input_topic_or_queue: String,
    #[structopt(long, env)]
    pub error_topic_or_exchange: String,
    #[structopt(long, env)]
    pub schema_registry_addr: String,
    #[structopt(long, env)]
    pub cache_capacity: usize,
    #[structopt(long, env, default_value = "128")]
    pub task_limit: usize,
    #[structopt(default_value = metrics::DEFAULT_PORT, env)]
    pub metrics_port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config: Config = Config::from_args();

    debug!("Environment {:?}", config);

    metrics::serve(config.metrics_port);

    let consumer = new_consumer(&config, &config.input_topic_or_queue).await?;
    let producer = Arc::new(new_producer(&config).await?);

    let cache = Arc::new(Mutex::new(LruCache::new(config.cache_capacity)));

    let error_topic_or_exchange = Arc::new(config.error_topic_or_exchange);
    let schema_registry_addr = Arc::new(config.schema_registry_addr);

    let task_limiter = TaskLimiter::new(config.task_limit);
    let task_queue = Arc::new(ParallelTaskQueue::default());

    consumer
        .par_run(
            Handler {
                cache,
                producer,
                error_topic_or_exchange,
                schema_registry_addr,
                task_queue,
            },
            task_limiter,
        )
        .await?;

    tokio::time::delay_for(tokio::time::Duration::from_secs(3)).await;

    Ok(())
}

struct Handler {
    cache: Arc<Mutex<LruCache<Uuid, String>>>,
    producer: Arc<CommonPublisher>,
    error_topic_or_exchange: Arc<String>,
    schema_registry_addr: Arc<String>,
    task_queue: Arc<ParallelTaskQueue>,
}

#[async_trait]
impl ParConsumerHandler for Handler {
    async fn handle<'a>(&'a self, message: &'a dyn CommunicationMessage) -> anyhow::Result<()> {
        let order_group_id = get_order_group_id(message);
        let _guard =
            order_group_id.map(move |x| async move { self.task_queue.acquire_permit(x).await });

        trace!("Received message `{:?}`", message.payload());

        let message_key = get_order_group_id(message).unwrap_or_default();
        counter!("cdl.data-router.input-msg", 1);
        let result = async {
            let json_something: Value = serde_json::from_str(message.payload()?)
                .context("Payload deserialization failed")?;
            if json_something.is_array() {
                trace!("Processing multimessage");

                let maybe_array: Vec<DataRouterInsertMessage> = serde_json::from_str(
                    message.payload()?,
                )
                .context("Payload deserialization failed, message is not a valid cdl message ")?;

                let mut result = Ok(());

                for entry in maybe_array.iter() {
                    let r = route(
                        &self.cache,
                        &entry,
                        &message_key,
                        &self.producer,
                        &self.schema_registry_addr,
                    )
                    .await
                    .context("Tried to send message and failed");

                    counter!("cdl.data-router.input-multimsg", 1);
                    counter!("cdl.data-router.processed", 1);

                    if r.is_err() {
                        result = r;
                    }
                }

                result
            } else {
                trace!("Processing single message");

                let owned: DataRouterInsertMessage =
                    serde_json::from_str::<DataRouterInsertMessage>(message.payload()?).context(
                        "Payload deserialization failed, message is not a valid cdl message",
                    )?;
                let result = route(
                    &self.cache,
                    &owned,
                    &message_key,
                    &self.producer,
                    &self.schema_registry_addr,
                )
                .await
                .context("Tried to send message and failed");
                counter!("cdl.data-router.input-singlemsg", 1);
                counter!("cdl.data-router.processed", 1);

                result
            }
        }
        .await;

        counter!("cdl.data-router.input-request", 1);

        if let Err(error) = result {
            counter!("cdl.data-router.error", 1);
            //TODO: Remove it.
            send_message(
                self.producer.as_ref(),
                &self.error_topic_or_exchange,
                SERVICE_NAME,
                format!("{:?}", error).into(),
            )
            .await;

            return Err(error);
        } else {
            counter!("cdl.data-router.success", 1);
        }

        Ok(())
    }
}

async fn new_producer(config: &Config) -> anyhow::Result<CommonPublisher> {
    Ok(match config.message_queue {
        MessageQueueKind::Kafka => {
            let brokers = config
                .kafka_brokers
                .as_ref()
                .context("kafka brokers were not specified")?;
            CommonPublisher::new_kafka(brokers).await?
        }
        MessageQueueKind::Amqp => {
            let connection_string = config
                .amqp_connection_string
                .as_ref()
                .context("amqp connection string was not specified")?;
            CommonPublisher::new_amqp(connection_string).await?
        }
    })
}

async fn new_consumer(config: &Config, topic_or_queue: &str) -> anyhow::Result<CommonConsumer> {
    let config = match config.message_queue {
        MessageQueueKind::Kafka => {
            let brokers = config
                .kafka_brokers
                .as_ref()
                .context("kafka brokers were not specified")?;
            let group_id = config
                .kafka_group_id
                .as_ref()
                .context("kafka group was not specified")?;

            debug!("Initializing Kafka consumer");

            CommonConsumerConfig::Kafka {
                brokers: &brokers,
                group_id: &group_id,
                topic: topic_or_queue,
            }
        }
        MessageQueueKind::Amqp => {
            let connection_string = config
                .amqp_connection_string
                .as_ref()
                .context("amqp connection string was not specified")?;
            let consumer_tag = config
                .amqp_consumer_tag
                .as_ref()
                .context("amqp consumer tag was not specified")?;

            debug!("Initializing Amqp consumer");

            CommonConsumerConfig::Amqp {
                connection_string: &connection_string,
                consumer_tag: &consumer_tag,
                queue_name: topic_or_queue,
                options: None,
            }
        }
    };
    Ok(CommonConsumer::new(config).await?)
}

async fn route(
    cache: &Mutex<LruCache<Uuid, String>>,
    event: &DataRouterInsertMessage<'_>,
    message_key: &str,
    producer: &CommonPublisher,
    schema_registry_addr: &str,
) -> anyhow::Result<()> {
    let payload = BorrowedInsertMessage {
        object_id: event.object_id,
        schema_id: event.schema_id,
        timestamp: current_timestamp(),
        data: event.data,
    };
    let topic_name = get_schema_topic(&cache, payload.schema_id, &schema_registry_addr).await?;

    send_message(
        producer,
        &topic_name,
        message_key,
        serde_json::to_vec(&payload)?,
    )
    .await;
    Ok(())
}

async fn get_schema_topic(
    cache: &Mutex<LruCache<Uuid, String>>,
    schema_id: Uuid,
    schema_addr: &str,
) -> anyhow::Result<String> {
    let recv_channel = cache
        .lock()
        .unwrap_or_else(abort_on_poison)
        .get_mut(&schema_id)
        .cloned();
    if let Some(val) = recv_channel {
        trace!("Retrieved topic for {} from cache", schema_id);
        return Ok(val);
    }

    let mut client = rpc::schema_registry::connect(schema_addr.to_owned()).await?;
    let channel = client
        .get_schema_topic(Id {
            id: schema_id.to_string(),
        })
        .await?
        .into_inner()
        .topic;

    trace!("Retrieved topic for {} from schema registry", schema_id);
    cache
        .lock()
        .unwrap_or_else(abort_on_poison)
        .insert(schema_id, channel.clone());

    Ok(channel)
}

async fn send_message(producer: &CommonPublisher, topic_name: &str, key: &str, payload: Vec<u8>) {
    let payload_len = payload.len();
    let delivery_status = producer.publish_message(&topic_name, key, payload).await;

    if delivery_status.is_err() {
        error!(
            "Fatal error, delivery status for message not received.  Topic: `{}`, Key: `{}`, Payload len: `{}`, {:?}",
            topic_name, key, payload_len, delivery_status
        );
        process::abort();
    };
    counter!("cdl.data-router.output-singleok", 1);
}
