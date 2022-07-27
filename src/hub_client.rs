use astarte_sdk::AstarteSdk;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use futures::stream;
use rand::rngs::ThreadRng;
use rand::Rng;
use tokio::time;
use tonic::codegen::InterceptedService;
use tonic::metadata::MetadataValue;
use tonic::service::Interceptor;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};

use msg_hub::msg_hub_client::MsgHubClient;
use msg_hub::{Interface, NodeData, NodeIntrospection};

pub mod msg_hub {
    tonic::include_proto!("msghub");
}

type HubClient = MsgHubClient<InterceptedService<Channel, MyInterceptor>>;

async fn run_attach(client: &mut HubClient) -> Result<(), Box<dyn Error>> {
    let interfaces = vec![
        Interface {
            name: "io.demo.ServerProperties".to_owned(),
            minor: 0,
            major: 2,
        },
        Interface {
            name: "org.astarteplatform.esp32.DeviceDatastream".to_owned(),
            minor: 0,
            major: 2,
        },
    ];
    let node_introspection = NodeIntrospection { interfaces };

    let mut stream = client
        .attach(Request::new(node_introspection))
        .await?
        .into_inner();

    while let Some(hub_msg) = stream.message().await? {
        println!("Msg = {:?}", hub_msg);
    }

    Ok(())
}

async fn send_int(client: &mut HubClient) -> Result<(), Box<dyn Error>> {
    use crate::msg_hub::{astarte_sdk_type::OneOfAstarteType, AstarteSdkType};
    use std::time::{SystemTime, UNIX_EPOCH};

    let node_message = msg_hub::NodeMessage {
        interface: "org.astarteplatform.esp32.DeviceDatastream".to_owned(),
        path: "/uptimeSeconds".to_owned(),
        aggregation_type: msg_hub::AstAggregation::Individual.into(),
        data: Some(AstarteSdkType {
            one_of_astarte_type: Some(OneOfAstarteType::Int32(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i32,
            )),
        }),
    };

    client.send_data(Request::new(node_message)).await?;

    Ok(())
}

struct MyInterceptor;

impl Interceptor for MyInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let node_id: MetadataValue<_> = "12345fag".parse().unwrap();
        let node_name: MetadataValue<_> = "node-1".parse().unwrap();

        request.metadata_mut().insert("node-id", node_id.clone());
        request
            .metadata_mut()
            .insert("node-name", node_name.clone());

        Ok(request)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let channel = Endpoint::from_static("http://[::1]:10000")
        .connect()
        .await?;

    let channel_clone = channel.clone();
    let mut client = MsgHubClient::with_interceptor(channel, MyInterceptor);
    let mut client_clone = MsgHubClient::with_interceptor(channel_clone, MyInterceptor);

    println!("\n*** ATTACH ***");

    tokio::spawn(async move {
        loop {
            use tokio::time::{sleep, Duration};
            send_int(&mut client_clone).await.unwrap();
            println!("\n*** SEND ***");
            sleep(Duration::from_secs(10)).await;
        }
    });

    run_attach(&mut client).await?;

    Ok(())
}
