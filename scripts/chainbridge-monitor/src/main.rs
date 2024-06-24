mod cli;
mod ethereum_listener;
mod influx_db_writer;
mod substrate_listener;
use anyhow::Result;
use cli::Cli;
use ethereum_listener::run_ethereum_listner;
use influx_db_writer::{run_influx_db_client, InfluxDbEvents};
use structopt::StructOpt;
use substrate_listener::run_substrate_listener;
use tokio::join;
use tokio::sync::mpsc::channel;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::from_args();
    let eth_args = args.clone();
    let influx_args = args.clone();

    let (sender, receiver) = channel::<InfluxDbEvents>(100);
    let eth_sender = sender.clone();
    let ethereum_listener_handler = tokio::spawn(async move {
        run_ethereum_listner(eth_sender, eth_args).await.unwrap();
    });
    let influx_db_handler = tokio::spawn(async move {
        run_influx_db_client(receiver, influx_args).await.unwrap();
    });
    run_substrate_listener(sender, args).await?;

    if let (Ok(()), Ok(())) = join!(ethereum_listener_handler, influx_db_handler,) {
        println!("All the jobs exited succesfully");
    }
    Ok(())
}
