pub mod command_service;
pub mod error;
pub mod query_service;
pub mod query_service_ts;
pub mod schema_registry;
pub mod generic {
    use crate::error::ClientError;
    use generic_rpc_client::GenericRpcClient;
    use tonic::transport::Channel;

    tonic::include_proto!("generic_rpc");

    pub async fn connect(
        addr: String,
        service: &'static str,
    ) -> Result<GenericRpcClient<Channel>, ClientError> {
        GenericRpcClient::connect(addr)
            .await
            .map_err(|err| ClientError::ConnectionError {
                service,
                source: err,
            })
    }
}

pub use tonic;
