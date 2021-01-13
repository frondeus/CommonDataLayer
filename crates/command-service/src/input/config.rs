pub enum InputConfig {
    MessageQueue(MessageQueueConfig),
    GRpc(GRpcConfig),
}

#[derive(Clone, Debug)]
pub struct MessageQueueConfig {
    pub consumer_tag: String,
    pub connection_string: String,
    pub ordered_queue_names: Vec<String>,
    pub unordered_queue_names: Vec<String>,
    pub task_limit: usize,
}

#[derive(Clone, Debug)]
pub struct GRpcConfig {
    pub grpc_port: u16,
}
