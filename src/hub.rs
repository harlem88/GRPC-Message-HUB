use std::collections::HashMap;
use std::sync::Arc;

use astarte_sdk::builder::AstarteOptions;
use astarte_sdk::types::AstarteType;
use astarte_sdk::AstarteSdk;
use structopt::StructOpt;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::{AsciiMetadataValue, KeyAndValueRef, MetadataValue};
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use msg_hub::msg_hub_server::{MsgHub, MsgHubServer};
use msg_hub::{NodeData, NodeIntrospection};

use crate::msg_hub::AstarteSdkType;
use crate::msg_hub::NodeMessage;

pub mod msg_hub {
    tonic::include_proto!("msghub");
}

struct Node {
    id: String,
    name: String,
    introspection: NodeIntrospection,
    node_channel: Sender<Result<NodeMessage, Status>>,
}

pub struct MsgHubService {
    nodes: Arc<Mutex<HashMap<String, Node>>>,
    device_sdk: AstarteSdk,
}

#[derive(Debug, StructOpt)]
struct Cli {
    // Realm name
    #[structopt(short, long)]
    realm: String,
    // Device id
    #[structopt(short, long)]
    device_id: String,
    // Credentials secret
    #[structopt(short, long)]
    credentials_secret: String,
    // Pairing URL
    #[structopt(short, long)]
    pairing_url: String,
}

#[tonic::async_trait]
impl MsgHub for MsgHubService {
    type AttachStream = ReceiverStream<Result<NodeMessage, Status>>;

    async fn attach(
        &self,
        request: Request<NodeIntrospection>,
    ) -> Result<Response<Self::AttachStream>, Status> {
        println!("attach => {:?}", request);

        let node_id = request
            .metadata()
            .get("node-id")
            .ok_or_else(|| AsciiMetadataValue::try_from(""))
            .unwrap()
            .to_str()
            .unwrap();
        let node_name = request
            .metadata()
            .get("node-name")
            .ok_or_else(|| AsciiMetadataValue::try_from(""))
            .unwrap()
            .to_str()
            .unwrap();

        let (tx, rx) = mpsc::channel(4);

        let node = Node {
            id: node_id.to_owned(),
            name: node_name.to_owned(),
            introspection: request.into_inner(),
            node_channel: tx,
        };

        let mut nodes = self.nodes.lock().await;
        nodes.insert(node.id.to_owned(), node);

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn send_data(&self, request: Request<NodeMessage>) -> Result<Response<NodeData>, Status> {
        let metadata = request.metadata().clone();
        let node_message = request.into_inner();

        let node_id = metadata
            .get("node-id")
            .ok_or_else(|| AsciiMetadataValue::try_from(""))
            .unwrap()
            .to_str()
            .unwrap();
        let node_name = metadata
            .get("node-name")
            .ok_or_else(|| AsciiMetadataValue::try_from(""))
            .unwrap()
            .to_str()
            .unwrap();

        let node_guard = self.nodes.lock().await;
        let can_publish = node_guard
            .get(node_id)
            .map(|node| {
                node.introspection
                    .interfaces
                    .iter()
                    .filter(|iface| iface.name == node_message.interface)
                    .count()
                    > 0
            })
            .is_some();

        if can_publish {
            let astarte_type: AstarteType = node_message.data.unwrap().try_into().unwrap();
            match node_message.aggregation_type {
                0 => {
                    self.device_sdk
                        .send(&node_message.interface, &node_message.path, astarte_type)
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }

        Ok(Response::new(NodeData {
            message: "".to_owned(),
        }))
    }

    async fn detach(
        &self,
        request: Request<NodeIntrospection>,
    ) -> Result<Response<NodeData>, Status> {
        Ok(Response::new(NodeData {
            message: "".to_owned(),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Cli {
        realm,
        device_id,
        credentials_secret,
        pairing_url,
    } = Cli::from_args();

    let addr = "[::1]:10000".parse().unwrap();

    println!("Message Node listening on: {}", addr);

    let sdk_options = AstarteOptions::new(&realm, &device_id, &credentials_secret, &pairing_url)
        .interface_directory("./interfaces")?
        .ignore_ssl_errors()
        .build();

    let mut device = AstarteSdk::new(&sdk_options).await?;
    let device_clone = device.clone();

    let msg_hub = MsgHubService {
        nodes: Arc::new(Mutex::new(HashMap::new())),
        device_sdk: device_clone,
    };

    let nodes = msg_hub.nodes.clone();
    let svc = MsgHubServer::new(msg_hub);

    tokio::spawn(async move {
        loop {
            match device.poll().await {
                Ok(data) => {
                    println!("incoming: {:?}", data);

                    if let astarte_sdk::Aggregation::Individual(var) = data.data {
                        let node_message = msg_hub::NodeMessage {
                            interface: data.interface.to_owned(),
                            path: data.path.to_owned(),
                            aggregation_type: msg_hub::AstAggregation::Individual.into(),
                            data: Some(var.try_into().unwrap()),
                        };

                        let node_guard = nodes.lock().await;
                        let node_selected: Vec<&Node> = node_guard
                            .iter()
                            .filter(|(id, node)| {
                                node.introspection
                                    .interfaces
                                    .iter()
                                    .filter(|iface| iface.name == data.interface)
                                    .count()
                                    > 0
                            })
                            .map(|(id, node)| node)
                            .collect();
                        for node in node_selected {
                            println!(" Node Found");
                            let result = node.node_channel.send(Ok(node_message.clone())).await;
                            if result.is_err() {
                                println!(" Channel closed");
                            }
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

impl TryFrom<AstarteType> for AstarteSdkType {
    type Error = String;

    fn try_from(value: AstarteType) -> Result<Self, Self::Error> {
        let one_of_value = match value {
            AstarteType::Double(value) => {
                Ok(msg_hub::astarte_sdk_type::OneOfAstarteType::Double(value))
            }
            AstarteType::Integer(value) => {
                Ok(msg_hub::astarte_sdk_type::OneOfAstarteType::Int32(value))
            }
            _ => Err("Can't convert to astarte".to_owned()),
        };

        Ok(msg_hub::AstarteSdkType {
            one_of_astarte_type: Some(one_of_value?),
        })
    }
}

impl TryFrom<AstarteSdkType> for AstarteType {
    type Error = String;

    fn try_from(value: AstarteSdkType) -> Result<Self, Self::Error> {
        if let Some(msg_hub::astarte_sdk_type::OneOfAstarteType::Double(dvalue)) =
            value.one_of_astarte_type
        {
            Ok(AstarteType::Double(dvalue))
        } else if let Some(msg_hub::astarte_sdk_type::OneOfAstarteType::Int32(dvalue)) =
            value.one_of_astarte_type
        {
            Ok(AstarteType::Integer(dvalue))
        } else {
            Err("Can't convert to astarte".to_owned())
        }
    }
}
