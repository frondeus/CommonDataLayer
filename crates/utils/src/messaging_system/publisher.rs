use lapin::{options::BasicPublishOptions, BasicProperties, Channel};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use reqwest::Client;
use std::time::Duration;
use tokio_amqp::LapinTokioExt;
use url::Url;

use super::Result;

#[derive(Clone)]
pub enum CommonPublisher {
    Kafka { producer: FutureProducer },
    Amqp { channel: Channel },
    Rest { url: Url, client: Client },
}
impl CommonPublisher {
    pub async fn new_amqp(connection_string: &str) -> Result<CommonPublisher> {
        let connection = lapin::Connection::connect(
            connection_string,
            lapin::ConnectionProperties::default().with_tokio(),
        )
        .await?;
        let channel = connection.create_channel().await?;

        Ok(CommonPublisher::Amqp { channel })
    }

    pub async fn new_kafka(brokers: &str) -> Result<CommonPublisher> {
        let publisher = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("acks", "all")
            .set("compression.type", "none")
            .set("max.in.flight.requests.per.connection", "1")
            .create()?;
        Ok(CommonPublisher::Kafka {
            producer: publisher,
        })
    }

    pub async fn new_rest(url: Url) -> Result<CommonPublisher> {
        Ok(CommonPublisher::Rest {
            url,
            client: reqwest::Client::new(),
        })
    }

    pub async fn publish_message(
        &self,
        topic_or_exchange: &str,
        key: &str,
        payload: Vec<u8>,
    ) -> Result<()> {
        match self {
            CommonPublisher::Kafka { producer } => {
                let delivery_status = producer.send(
                    FutureRecord::to(topic_or_exchange)
                        .payload(&payload)
                        .key(key),
                    Duration::from_secs(5),
                );
                delivery_status.await.map_err(|x| x.0)?;
                Ok(())
            }
            CommonPublisher::Amqp { channel } => {
                channel
                    .basic_publish(
                        topic_or_exchange,
                        key,
                        BasicPublishOptions::default(),
                        payload,
                        BasicProperties::default().with_delivery_mode(2), // persistent messages
                    )
                    .await?
                    .await?;
                Ok(())
            }
            CommonPublisher::Rest { url, client } => {
                let url = url.join(&format!("{}/{}", topic_or_exchange, key)).unwrap();

                client.post(url).body(payload).send().await?;

                Ok(())
            }
        }
    }
}
