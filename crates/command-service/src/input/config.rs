pub enum InputConfig {
    Kafka(MessageQueueConfig),
    GRpc(GRpcConfig),
}

#[derive(Clone, Debug)]
pub struct MessageQueueConfig {
    pub consumer_tag: String,
    pub connection_string: String,
    pub queue_names: Vec<String>,
    pub task_limit: usize,
}

#[derive(Clone, Debug)]
pub struct GRpcConfig {
    pub grpc_port: u16,
}
