use learn_asyn_rust::chapters::{ch6};

//tokio::main works like block_on function that we created in ch4
#[tokio::main]
async fn main() {
    //ch6::tokio_spawn().await;
    ch6::tokio_channels().await;
}