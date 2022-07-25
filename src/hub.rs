use astarte_sdk::builder::AstarteOptions;
use astarte_sdk::AstarteSdk;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::{Stream, StreamExt, TryFutureExt};
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, Mutex};
use tokio_rustls::rustls::internal::msgs::message::Message;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::msg_hub::HubMessage;
use msg_hub::msg_hub_server::{MsgHub, MsgHubServer};
use msg_hub::{Device, HubData, Interface};

pub mod msg_hub {
    tonic::include_proto!("msghub");
}

struct Node {
    node_channel: Sender<Result<HubMessage, Status>>,
    device: Device,
}

pub struct MsgHubService {
    nodes: Arc<Mutex<HashMap<i32, Node>>>,
    deviceSDK: AstarteSdk,
}

#[tonic::async_trait]
impl MsgHub for MsgHubService {
    type AttachStream = ReceiverStream<Result<HubMessage, Status>>;

    async fn attach(
        &self,
        request: Request<Device>,
    ) -> Result<Response<Self::AttachStream>, Status> {
        println!("attach => {:?}", request);

        let (tx, rx) = mpsc::channel(4);

        let node = Node {
            node_channel: tx,
            device: request.into_inner(),
        };

        let mut nodes = self.nodes.lock().await;
        nodes.entry(node.device.id).or_insert(node);

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

    let mut sdk_options = AstarteOptions::new(
        "&opts.realm",
        "&device_id",
        "&credentials_secret",
        "&opts.pairing_url",
    );

    let sdk_options = sdk_options
        .interface_directory("&opts.interfaces_directory")?
        .build();

    let mut device = AstarteSdk::new(&sdk_options).await?;
    let device_clone = device.clone();

    let msg_hub = MsgHubService {
        nodes: Arc::new(Mutex::new(HashMap::new())),
        deviceSDK: device_clone,
    };

    let nodes = msg_hub.nodes.clone();
    let svc = MsgHubServer::new(msg_hub);

    tokio::spawn(async move {
        loop {
            match device.poll().await {
                Ok(data) => {
                    println!("incoming: {:?}", data);

                    let hub_message = msg_hub::HubMessage {
                        interface: data.interface.to_owned(),
                        path: data.path.to_owned(),
                        aggregation_type: msg_hub::AstAggregation::Individual.into(),
                        data: Some(msg_hub::AstarteSdkType {
                            one_of_astarte_type: Some(
                                msg_hub::astarte_sdk_type::OneOfAstarteType::Int32(33),
                            ),
                        }),
                    };

                    //find which nodes contains the interface

                    let node_guard = nodes.lock().await;
                    let node = node_guard.get(&123);
                    if let Some(node) = node {
                        println!(" Node Found");

                        let result = node.node_channel.send(Ok(hub_message)).await;

                        if result.is_err() {
                            println!(" Channel closed");
                        }
                    }
                }
                Err(err) => println!("{:?}", err),
            }
        }
    });

    let serve = Server::builder().add_service(svc).serve(addr).await?;

    Ok(())
}
