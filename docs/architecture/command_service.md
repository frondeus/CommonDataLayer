# Command Services
Services that translate messages received from the [Data Router][data-router] into their respective database's format. Currently only one Command Service implementation exists
and is built in such way that it can support multiple databases (one at a time).

### Technical Description

The Command-Service (commonly refered also as `CS`, or `CSPG` - indicating posgres instance), interfaces storage repositories with the CDL ecosystem.

Interacts with:
- Data Router (optional, either)
- Message Queue (optional, either)
- Supported Repository (one of)

Ingest methods:
- Kafka
- RabbitMq
- GRPC (currently either only one instance without kubernetes)

Egest methods (supported repositories):
- Postgresql (tested on 12, should support anything >=9, advised 13)
- VictoriaMetrics
- Druid
- Sleight (CDL's document storage)
- Troika (CDL's binary data repo)
- .. or anything with matching GRPC :)

### Configuration (environment files)

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| COMMUNICATION_METHOD | The method to communicate with external services | `kafka` / `amqp` / `grpc` | yes | |
| REPORT_DESTINATION | When missing reporting service is disabled. Kafka topic / amqp exchange or REST parameter | `cdl.notifications` | no | |
| METRICS_PORT | Port to listen on prometheus requests | 58105 | no | 58105 |
| RUST_LOG | Log level | `trace` | no | |

#### Postgres configuration

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| POSTGRES_USERNAME | Username | `cdl` | yes | |
| POSTGRES_PASSWORD | Password | `cdl1234` | yes | |
| POSTGRES_HOST | Host of the server | `127.0.0.1` | yes | |
| POSTGRES_PORT | Port on which the server listens | 5432 | yes | |
| POSTGRES_DBNAME | Database name | `cdl` | yes | |
| POSTGRES_SCHEMA | SQL Schema available for service | `cdl` | no | `public` |

#### Druid configuration

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| DRUID_OUTPUT_BROKERS | Kafka brokers | `kafka:9093` | yes | |
| DRUID_OUTPUT_TOPIC | Kafka topic | `cdl.timeseries.internal.druid` | yes | |

#### Victoria metrics configuration
| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| VICTORIA_METRICS_OUTPUT_URL | Url to the VM | `http://victoria_metrics:8428` | yes | |

#### Kafka configuration 
(if `COMMUNICATION_METHOD` equals `kafka`)

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| KAFKA_BROKERS | Address to kafka brokers | `kafka:9093` | yes | |
| KAFKA_GROUP_ID | Group id of the consumer | `postgres_command` | yes | |
| ORDERED_SOURCES | topics with ordered messages | `cdl.timeseries.vm.1.data` | no, but one of `ORDERED_SOURCES` and `UNORDERED_SOURCES` has to be present | |
| UNORDERED_SOURCES | topics with unordered messages | `cdl.timeseries.vm.2.data` | no, but one of `ORDERED_SOURCES` and `UNORDERED_SOURCES` has to be present | |
| TASK_LIMIT | limits the number of tasks | 32 | yes | 32 |

#### Amqp configuration 
(if `COMMUNICATION_METHOD` equals `amqp`)

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| AMQP_CONNECTION_STRING | Connection string to AMQP Server | `amqp://user:CHANGEME@rabbitmq:5672/%2f` | yes | |
| AMQP_CONSUMER_TAG | Consumer tag | `postgres_command` | yes | |
| ORDERED_SOURCES | queus with ordered messages | `cdl.timeseries.vm.1.data` | no, but one of `ORDERED_SOURCES` and `UNORDERED_SOURCES` has to be present | |
| UNORDERED_SOURCES | queues with unordered messages | `cdl.timeseries.vm.2.data` | no, but one of `ORDERED_SOURCES` and `UNORDERED_SOURCES` has to be present | |
| TASK_LIMIT | limits the number of tasks | 32 | yes | 32 |

#### GRPC configuration 
(if `COMMUNICATION_METHOD` equals `grpc`)

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
|
| GRPC_PORT | Port to listen on | 50103 | yes | |
| REPORT_ENDPOINT_URL | Url to send notifications on | `notifications:50102` | yes | |

[data-router]: data_router.md
