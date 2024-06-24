use anyhow::Result;
use ethers::{
    contract::{abigen, LogMeta},
    providers::{Provider, StreamExt, Ws},
    types::H160,
};
use sp_core::H256;
use std::str::FromStr;
use std::sync::Arc;

use crate::{cli::Cli, influx_db_writer::InfluxDbEvents};

abigen!(ERC20HandlerContract, "./contract_abi/ERC20Handler.json");
abigen!(BridgeContract, "./contract_abi/Bridge.json");

#[derive(Debug)]
pub struct DepositRequest {
    pub recipient: String,
    pub sender: String,
    pub nonce: u64,
    pub amount: u64,
    pub tx_hash: H256,
    pub block_number: u64,
}

impl DepositRequest {
    pub fn new(
        recipient: String,
        sender: String,
        amount: u64,
        nonce: u64,
        tx_hash: H256,
        block_number: u64,
    ) -> Self {
        DepositRequest {
            recipient,
            sender,
            amount,
            nonce,
            tx_hash,
            block_number,
        }
    }
}

// Listens to smart contract deposit events
pub async fn run_ethereum_listner(
    sender: tokio::sync::mpsc::Sender<InfluxDbEvents>,
    cli: Cli,
) -> Result<()> {
    subscribe_deposit_events(sender, cli).await?;
    Ok(())
}

pub async fn subscribe_deposit_events(
    sender: tokio::sync::mpsc::Sender<InfluxDbEvents>,
    cli: Cli,
) -> Result<()> {
    let (eth_endpoint, handler_contract, bridge_contract) = (
        cli.eth_endpoint,
        cli.handler_contract_address,
        cli.bridge_contract_address,
    );
    let bridge_address = H160::from_str(bridge_contract.clone().as_str())?;
    let provider = Provider::<Ws>::connect(eth_endpoint.clone()).await?;
    let client = Arc::new(provider);
    let bridge_contract = BridgeContract::new(bridge_address, client);

    let events = bridge_contract.event::<DepositFilter>();
    println!("Subscribing Deposit Events");
    let mut stream = events.stream().await?.with_meta();
    while let Some(Ok((event, meta))) = stream.next().await {
        let sender = sender.clone();
        println!("Received Deposit event: {:?}", event);
        println!("Received Metadata: {:?}", meta);
        handle_deposit_event(
            event,
            meta,
            sender,
            handler_contract.clone(),
            eth_endpoint.clone(),
        )
        .await?;
    }
    Ok(())
}

pub async fn handle_deposit_event(
    deposit: DepositFilter,
    meta: LogMeta,
    sender: tokio::sync::mpsc::Sender<InfluxDbEvents>,
    handler_contract: String,
    eth_endpoint: String,
) -> Result<()> {
    let provider = Provider::<Ws>::connect(eth_endpoint).await?;
    let client = Arc::new(provider);
    let address = H160::from_str(handler_contract.as_str())?;
    let contract = ERC20HandlerContract::new(address, client);
    println!("Fetching deposit records from ERC20Handler");
    let deposit_record: DepositRecord = contract
        .get_deposit_record(deposit.deposit_nonce, deposit.destination_chain_id)
        .call()
        .await?;
    println!(
        "The receiver is: {:?}, amount: {:?}, nonce: {:?}",
        deposit_record.destination_recipient_address, deposit_record.amount, deposit.deposit_nonce
    );

    let receiver = deposit_record.destination_recipient_address;
    let amount: u64 = deposit_record.amount.as_u64();
    let tx_hash = meta.transaction_hash;
    let block_number = meta.block_number;

    let request = DepositRequest::new(
        receiver.to_string(),
        receiver.to_string(),
        amount,
        deposit.deposit_nonce,
        tx_hash,
        block_number.as_u64(),
    );
    println!("Received New Deposit Request: {:?}", request);
    sender
        .send(InfluxDbEvents::EthereumDepositRequested(request))
        .await?;
    println!("Sent the Deposit Request to the Influx Db client");
    Ok(())
}
