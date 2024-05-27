use anyhow::{anyhow, Result};
use parity_scale_codec::{Decode, Encode};
use sp_core::crypto::AccountId32;
pub use substrate_api_client::{
    ac_node_api::StaticEvent, ac_primitives::config::DefaultRuntimeConfig, rpc::JsonrpseeClient,
    Api, SubscribeEvents,
};

pub use DefaultRuntimeConfig as ParentchainRuntimeConfig;

use crate::{cli::Cli, influx_db_writer::InfluxDbEvents};
pub type ParentchainApi = Api<ParentchainRuntimeConfig, JsonrpseeClient>;

pub struct DepositCompleted {
    pub deposit_nonce: DepositNonce,
    pub block_hash: String,
}

impl DepositCompleted {
    pub fn new(deposit_nonce: DepositNonce, block_hash: String) -> Self {
        Self {
            deposit_nonce,
            block_hash,
        }
    }
}

pub type BridgeChainId = u8;
pub type DepositNonce = u64;
pub type Balance = u128;

#[derive(Encode, Decode, Debug)]
pub struct NativeTokenMinted {
    pub to: AccountId32,
    pub amount: Balance,
}

impl StaticEvent for NativeTokenMinted {
    const PALLET: &'static str = "BridgeTransfer";
    const EVENT: &'static str = "NativeTokenMinted";
}

#[derive(Encode, Decode, Debug)]
pub struct ProposalSucceeded {
    pub bridge_chain_id: BridgeChainId,
    pub deposit_nonce: DepositNonce,
}

impl StaticEvent for ProposalSucceeded {
    const PALLET: &'static str = "ChainBridge";
    const EVENT: &'static str = "ProposalSucceeded";
}

pub async fn run_substrate_listener(
    sender: tokio::sync::mpsc::Sender<InfluxDbEvents>,
    cli: Cli,
) -> Result<()> {
    let client = JsonrpseeClient::new(cli.litentry_endpoint.as_str())
        .await
        .map_err(|e| anyhow!("Failed to create JSON RPC Client: {:?}", e))?;

    let api = ParentchainApi::new(client)
        .await
        .map_err(|e| anyhow!("Failed to create Parentchain API: {:?}", e))?;

    let mut subscription = api
        .subscribe_events()
        .await
        .map_err(|e| anyhow!("Failed to subscribe to susbstrate events: {:?}", e))?;

    // Infinite loop for listening to event notifications from the stream
    println!("Setting poll for events from substrate");
    loop {
        let events = subscription
            .next_events_from_metadata()
            .await
            .expect("event poll returned None")
            .map_err(|e| anyhow!("Failed to get polled events: {:?}", e))
            .unwrap();

        let block_hash = events.block_hash();
        for event in events.iter() {
            if let Ok(event) = event {
                let pallet_name = event.pallet_name();
                match pallet_name {
                    "ChainBridge" => match event.variant_name() {
                        "ProposalSucceeded" => {
                            if let Ok(Some(ev)) = event.as_event::<ProposalSucceeded>() {
                                println!("Proposal Succeeded event: {:?}", ev);
                                let deposit_completed =
                                    DepositCompleted::new(ev.deposit_nonce, block_hash.to_string());
                                sender
                                    .send(InfluxDbEvents::SubstrateDepositCompleted(
                                        deposit_completed,
                                    ))
                                    .await
                                    .map_err(|e| {
                                        anyhow!("Failed to send message to influx_db: {:?}", e)
                                    })?;
                            }
                        }
                        _ => continue,
                    },
                    "BridgeTransfer" => match event.variant_name() {
                        "NativeTokenMinted" => {
                            if let Ok(Some(ev)) = event.as_event::<NativeTokenMinted>() {
                                println!(
                                    "ERC20 Minted Succesfully: Receiver: {:?}, Amount: {:?}",
                                    ev.to, ev.amount
                                )
                            }
                        }
                        _ => continue,
                    },
                    _ => {
                        continue;
                    }
                }
            }
        }
    }
}
