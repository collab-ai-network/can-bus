use structopt::StructOpt;

#[derive(StructOpt, Clone)]
pub struct Cli {
    /// The bridge contract address
    #[structopt(long)]
    pub bridge_contract_address: String,
    /// The ERC-20 Handler contract address
    #[structopt(long)]
    pub handler_contract_address: String,
    /// Litentry Websocket endpoint
    #[structopt(long)]
    pub litentry_endpoint: String,
    /// Ethereum Websocket endpoint
    #[structopt(long)]
    pub eth_endpoint: String,
    /// Influx DB endpoint for pushing events
    #[structopt(long)]
    pub influx_db_endpoint: String,
    /// Influx DB auth token
    #[structopt(long)]
    pub influx_db_auth_token: String,
    /// Influx DB bucket name
    #[structopt(long)]
    pub influx_db_bucket: String,
}
