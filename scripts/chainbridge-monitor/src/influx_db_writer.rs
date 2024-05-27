use crate::{cli::Cli, ethereum_listener::DepositRequest, substrate_listener::DepositCompleted};
use chrono::{DateTime, Utc};
use influxdb::{Client, Error, InfluxDbWriteable};

#[derive(InfluxDbWriteable)]
struct EthereumDepositRecord {
    time: DateTime<Utc>,
    #[influxdb(tag)]
    recipient: String,
    #[influxdb(tag)]
    sender: String,
    deposit_nonce: u64,
    amount: u64, // using f64 to handle potential decimal values for amounts
    #[influxdb(tag)]
    tx_hash: String,
    block_number: u64,
}

#[derive(InfluxDbWriteable, Debug)]
struct SubstrateDepositRecord {
    time: DateTime<Utc>,
    deposit_nonce: u64,
    #[influxdb(tag)]
    block_hash: String,
}

impl SubstrateDepositRecord {
    pub fn new(deposit_nonce: u64, block_hash: String) -> Self {
        Self {
            time: Utc::now(),
            deposit_nonce,
            block_hash,
        }
    }
}

pub enum InfluxDbEvents {
    EthereumDepositRequested(DepositRequest),
    SubstrateDepositCompleted(DepositCompleted),
}

impl EthereumDepositRecord {
    pub fn request_to_record(request: DepositRequest) -> Self {
        EthereumDepositRecord {
            time: Utc::now(),
            recipient: request.recipient,
            sender: request.sender,
            amount: request.amount,
            deposit_nonce: request.nonce,
            tx_hash: request.tx_hash.to_string(),
            block_number: request.block_number,
        }
    }
}

pub async fn run_influx_db_client(
    mut receiver: tokio::sync::mpsc::Receiver<InfluxDbEvents>,
    cli: Cli,
) -> Result<(), Error> {
    let client = Client::new(cli.influx_db_endpoint, cli.influx_db_bucket)
        .with_token(cli.influx_db_auth_token);
    while let Some(influx_db_events) = receiver.recv().await {
        match influx_db_events {
            InfluxDbEvents::EthereumDepositRequested(deposit_request) => {
                let deposit = EthereumDepositRecord::request_to_record(deposit_request);
                client.query(deposit.into_query("ethereum_deposits")).await?;
            }
            InfluxDbEvents::SubstrateDepositCompleted(deposit_completed) => {
                let deposit = SubstrateDepositRecord::new(
                    deposit_completed.deposit_nonce,
                    deposit_completed.block_hash,
                );
                client.query(deposit.into_query("substrate_deposits")).await?;
            }
        }
    }
    Ok(())
}
