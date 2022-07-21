use std::sync::Arc;
use std::time::Instant;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_rustls::rustls::internal::msgs::message::Message;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use msg_hub::msg_hub_server::{MsgHub, MsgHubServer};
use msg_hub::{Device, HubData, Interface};

pub mod msg_hub {
    tonic::include_proto!("msghub");
}

#[derive(Debug)]
pub struct MsgHubService {
    devices: Arc<Vec<Device>>,
}

#[tonic::async_trait]
impl MsgHub for MsgHubService {
    type AttachStream = ReceiverStream<Result<HubData, Status>>;

    async fn attach(
        &self,
        request: Request<Device>,
    ) -> Result<Response<Self::AttachStream>, Status> {
        println!("attach => {:?}", request);

        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let mut i = 0;
            let mut channel_result = tx
                .send(Ok(HubData {
                    message: format!("msg {} ", i).to_owned(),
                }))
                .await;
            while channel_result.is_ok() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                println!("  => send {:?}", i);
                i += 1;
                channel_result = tx
                    .send(Ok(HubData {
                        message: format!("msg {} ", i).to_owned(),
                    }))
                    .await;
            }
            println!(" Channel closed => {:?}", request);
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn detach(&self, request: Request<Device>) -> Result<Response<HubData>, Status> {
        Ok(Response::new(HubData {
            message: "".to_owned(),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:10000".parse().unwrap();

    println!("Message Hub listening on: {}", addr);

    let msg_hub = MsgHubService {
        devices: Arc::new(vec![]),
    };

    let svc = MsgHubServer::new(msg_hub);

    Server::builder().add_service(svc).serve(addr).await?;

    Ok(())
}
