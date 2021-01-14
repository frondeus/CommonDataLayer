use anyhow::Context;
use log::error;
use lru_cache::LruCache;
use rpc::schema_registry::Id;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    process,
    sync::{Arc, Mutex},
};
use structopt::StructOpt;
use tokio::pin;
use tokio::stream::StreamExt;
use utils::message_types::{DataRouterInsertMessage};
use utils::{
    abort_on_poison,
    message_types::BorrowedInsertMessage,
    messaging_system::{
        consumer::CommonConsumer, message::CommunicationMessage, publisher::CommonPublisher,
    },
    metrics::{self, counter},
};
use uuid::Uuid;

const SERVICE_NAME: &str = "data-router";

#[derive(StructOpt, Deserialize, Debug, Serialize)]
struct Config {
    #[structopt(long, env)]
    pub kafka_group_id: String,
    #[structopt(long, env)]
    pub kafka_topic: String,
    #[structopt(long, env)]
    pub kafka_brokers: String,
    #[structopt(long, env)]
    pub kafka_error_channel: String,
    #[structopt(long, env)]
    pub schema_registry_addr: String,
    #[structopt(long, env)]
    pub cache_capacity: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config: Config = Config::from_args();
    metrics::serve();

    let consumer = CommonConsumer::new_kafka(
        &config.kafka_group_id,
        &config.kafka_brokers,
        &[&config.kafka_topic],
    )
    .await?;
    let producer = Arc::new(
        CommonPublisher::new_kafka(&config.kafka_brokers)
            .await
            .unwrap(),
    );
    let cache = Arc::new(Mutex::new(LruCache::new(config.cache_capacity)));
    let consumer = consumer.leak();
    let message_stream = consumer.consume().await;
    pin!(message_stream);

    let kafka_error_channel = Arc::new(config.kafka_error_channel);
    let schema_registry_addr = Arc::new(config.schema_registry_addr);

    while let Some(message) = message_stream.next().await {
        match message {
            Ok(message) => {
                tokio::spawn(handle_message(
                    message,
                    cache.clone(),
                    producer.clone(),
                    kafka_error_channel.clone(),
                    schema_registry_addr.clone(),
                ));
            }
            Err(error) => {
                error!("Error fetching data from message queue {:?}", error);
                // no error handling necessary - message won't be acked - it was never delivered properly
            }
        };
    }
    Ok(())
}

async fn handle_message(
    message: Box<dyn CommunicationMessage>,
    cache: Arc<Mutex<LruCache<Uuid, String>>>,
    producer: Arc<CommonPublisher>,
    kafka_error_channel: Arc<String>,
    schema_registry_addr: Arc<String>,
) {
    counter!("cdl.data-router.input-msg", 1);
    let result: anyhow::Result<()> = async {
        let json_something: Value =
            serde_json::from_str(message.payload()?)
            .context("Payload deserialization failed")?;
        if json_something.is_array() {
            let maybe_array: Vec<DataRouterInsertMessage> = serde_json::from_str(message.payload()?)
                .context(
                "Payload deserialization failed, message is not a valid cdl message ",
            )?;
            for entry in maybe_array.iter() {
                route(&cache, &entry, &producer, &schema_registry_addr)
                    .await
                    .context("Tried to send message and failed")?;
                counter!("cdl.data-router.input-multimsg", 1);
            }
        } else {
            let owned : DataRouterInsertMessage = serde_json::from_str::<DataRouterInsertMessage>(message.payload()?)
                 .context("Payload deserialization failed, message is not a valid cdl message")?;
            route(&cache, &owned, &producer, &schema_registry_addr)
                .await
                .context("Tried to send message and failed")?;
            counter!("cdl.data-router.input-singlemsg", 1);
        }
        Ok(())
    }
    .await;

    counter!("cdl.data-router.input-request", 1);

    if let Err(error) = result {
        error!("{:?}", error);
        send_message(
            producer.as_ref(),
            &kafka_error_channel,
            SERVICE_NAME,
            format!("{:?}", error).into(),
        )
        .await;
    }
    let ack_result = message.ack().await;
    if let Err(e) = ack_result {
        error!(
            "Fatal error, delivery status for message not received. {:?}",
            e
        );
        process::abort();
    }
}

async fn route(
    cache: &Mutex<LruCache<Uuid, String>>,
    // event: Box<DataRouterInsertMessage<'_>,
    event: &DataRouterInsertMessage<'_>,
    producer: &CommonPublisher,
    schema_registry_addr: &String,
) -> anyhow::Result<()> {

    // let single_msg =  serde_json::from_value::<DataRouterInsertMessage>(*entry)
    //     .context("Payload deserialization failed, message is not a valid cdl message")?;
    // let payload = serde_json::from_str::<DataRouterInsertMessage>(event.clone())
    //     .context("Payload deserialization failed, message is not a valid cdl message")?;
    let payload = BorrowedInsertMessage {
        object_id: event.object_id,
        schema_id: event.schema_id,
        timestamp: current_timestamp(),
        data: event.data,
    };
    let topic_name = get_schema_topic(&cache, payload.schema_id, &schema_registry_addr).await?;

    let key = payload.object_id.to_string();
    send_message(
        producer,
        &topic_name,
        &key,
        serde_json::to_vec(&payload)?,
    )
    .await;
    counter!("cdl.data-router.input-single-msg", 1);
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
        counter!("cdl.data-router.output-sendabort", 1);
        process::abort();
    };
        counter!("cdl.data-router.output-singleok", 1);
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as i64
}
