use std::time::Duration;

use anyhow::Context;
use rdkafka::{producer::BaseProducer, ClientConfig};
use tokio_amqp::LapinTokioExt;

use super::Result;

pub enum MetadataFetcher {
    Kafka { producer: BaseProducer },
    Amqp { connection: lapin::Connection },
}

impl MetadataFetcher {
    pub async fn new_kafka(brokers: &str) -> Result<Self> {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .create()
            .context("Metadata fetcher creation failed")?;

        Ok(Self::Kafka { producer })
    }

    pub async fn new_amqp(connection_string: &str) -> Result<Self> {
        let connection = lapin::Connection::connect(
            connection_string,
            lapin::ConnectionProperties::default().with_tokio(),
        )
        .await
        .context("Metadata fetcher creation failed")?;

        Ok(Self::Amqp { connection })
    }

    pub async fn topic_or_exchange_exists(&self, topic_or_exchange: &str) -> Result<bool> {
        let owned_topic_or_exchange = String::from(topic_or_exchange);

        match self {
            MetadataFetcher::Amqp { connection } => {
                let channel: lapin::Channel = connection
                    .create_channel()
                    .await
                    .context("Metadata fetcher AMQP channel creation failed")?;
                let result = channel
                    .exchange_declare(
                        topic_or_exchange,
                        lapin::ExchangeKind::Topic,
                        lapin::options::ExchangeDeclareOptions {
                            passive: true,
                            ..Default::default()
                        },
                        Default::default(),
                    )
                    .await;

                match result {
                    Err(lapin::Error::ProtocolError(amqp_error)) => {
                        if let lapin::protocol::AMQPErrorKind::Soft(
                            lapin::protocol::AMQPSoftError::NOTFOUND,
                        ) = amqp_error.kind()
                        {
                            Ok(false)
                        } else {
                            Err(lapin::Error::ProtocolError(amqp_error).into())
                        }
                    }
                    Err(e) => Err(e.into()),
                    Ok(()) => Ok(true),
                }
            }
            MetadataFetcher::Kafka { producer } => {
                let producer = producer.clone();
                let metadata = tokio::task::spawn_blocking(move || {
                    let client = producer.client();
                    client.fetch_metadata(Some(&owned_topic_or_exchange), Duration::from_secs(5))
                })
                .await??;

                Ok(metadata
                    .topics()
                    .iter()
                    .map(|topic| topic.name())
                    .any(|name| name == topic_or_exchange))
            }
        }
    }
}
