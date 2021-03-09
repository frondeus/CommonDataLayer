# GraphQL API

Server which provides `/graphql` and `/graphiql` routes for CDL management.
It is self-describing, interactive and easy to use way to manage your instance.

# Getting started on local machine (via docker-compose)

Check our [guide](../deployment/local/docker-compose.md) to see how to deploy API locally.

You can access interactive graphQL editor at http://localhost:50106/graphiql. It supports auto-completion, has built-in documentation explorer and history. 

Because our schema-registry in docker-compose is automatically initialized with some schemas, you can start making queries right away, like:

``` graphql
{
    schemas {
      id,
      definitions {
        version,
        definition
      },
      views {
        expression
      }
    }
}
```

### Configuration


| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| INPUT_PORT | Port to listen on | 50103 | yes | |
| SCHEMA_REGISTRY_ADDR | Address of setup schema registry | http://schema_registry:50101 | yes | |
| QUERY_ROUTER_ADDR | Address of query router | http://query_router:50101 | yes | |
| COMMUNICATION_METHOD | The method to communicate with external services | `kafka` / `amqp` / `grpc` | yes | |
| RUST_LOG | Log level | `trace` | no | |

#### Kafka configuration
(if `COMMUNICATION_METHOD` equals `kafka`)

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| KAFKA_BROKERS | Address to kafka brokers | `kafka:9093` | yes | |
| KAFKA_GROUP_ID | Group id of the consumer | `postgres_command` | yes | |
| REPORT_SOURCE | kafka topic on which API listens for notifications | `cdl.notifications` | yes | |
| INSERT_DESTINATION | kafka topic to which API inserts new objects | `cdl.data.input` | yes | |

#### Amqp configuration 
(if `COMMUNICATION_METHOD` equals `amqp`)

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| AMQP_CONNECTION_STRING | Connection string to AMQP Server | `amqp://user:CHANGEME@rabbitmq:5672/%2f` | yes | |
| AMQP_CONSUMER_TAG | Consumer tag | `postgres_command` | yes | |
| REPORT_SOURCE | amqp queue on which API listens for notifications | `cdl.notifications` | yes | |
| INSERT_DESTINATION | amqp exchange to which API inserts new objects | `cdl.data.input` | yes | |

#### GRPC configuration 
(if `COMMUNICATION_METHOD` equals `grpc`)

| Name | Short Description | Example | Mandatory | Default |
|---|---|---|---|---|
| INSERT_DESTINATION | GRPC Service address on which API inserts new objects | `http://data_router:50101` | yes | |
