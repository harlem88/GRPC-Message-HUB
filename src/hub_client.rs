use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use futures::stream;
use rand::rngs::ThreadRng;
use rand::Rng;
use tokio::time;
use tonic::transport::Channel;
use tonic::Request;

use msg_hub::msg_hub_client::MsgHubClient;
use msg_hub::{Device, HubData, Interface};

pub mod msg_hub {
    tonic::include_proto!("msghub");
}

async fn run_attach(client: &mut MsgHubClient<Channel>) -> Result<(), Box<dyn Error>> {
    let interfaces = vec![Interface {
        name: "io.demo.ServerProperties".to_owned(),
        minor: 0,
        major: 2,
    }];
    let device = Device {
        id: 123,
        name: "device123".to_owned(),
        interfaces,
    };

    let mut stream = client.attach(Request::new(device)).await?.into_inner();

    while let Some(hub_msg) = stream.message().await? {
        println!("Msg = {:?}", hub_msg);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = MsgHubClient::connect("http://[::1]:10000").await?;

    println!("\n*** ATTACH ***");
    run_attach(&mut client).await?;

    Ok(())
}
//
// fn random_point(rng: &mut ThreadRng) -> Point {
//     let latitude = (rng.gen_range(0..180) - 90) * 10_000_000;
//     let longitude = (rng.gen_range(0..360) - 180) * 10_000_000;
//     Point {
//         latitude,
//         longitude,
//     }
// }
