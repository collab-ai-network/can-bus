use crate::{chain_spec, evm_tracing_types::EthApiOptions};
use clap::Parser;

/// Sub-commands supported by the collator.
#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Sub-commands concerned with benchmarking.
	/// The pallet benchmarking moved to the `pallet` sub-command.
	#[command(subcommand)]
	Benchmark(Box<frame_benchmarking_cli::BenchmarkCmd>),

	/// Try some testing command against a specified runtime state.
	#[cfg(feature = "try-runtime")]
	TryRuntime(try_runtime_cli::TryRuntimeCmd),

	/// Errors since the binary was not build with `--features try-runtime`.
	#[cfg(not(feature = "try-runtime"))]
	TryRuntime,
}

#[derive(Debug, Parser)]
#[command(
	propagate_version = true,
	args_conflicts_with_subcommands = true,
	subcommand_negates_reqs = true
)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[command(flatten)]
	pub run: sc_cli::RunCmd,

	/// Disable automatic hardware benchmarks.
	///
	/// By default these benchmarks are automatically ran at startup and measure
	/// the CPU speed, the memory bandwidth and the disk speed.
	///
	/// The results are then printed out in the logs, and also sent as part of
	/// telemetry, if telemetry is enabled.
	#[arg(long)]
	pub no_hardware_benchmarks: bool,

	/// Relay chain arguments
	#[arg(raw = true, conflicts_with = "relay-chain-rpc-url")]
	pub relay_chain_args: Vec<String>,

	#[clap(flatten)]
	pub eth_api_options: EthApiOptions,

	/// Enable Ethereum compatible JSON-RPC servers (disabled by default).
	#[clap(name = "enable-evm-rpc", long)]
	pub enable_evm_rpc: bool,

	/// Proposer's maximum block size limit in bytes
	#[clap(long, default_value = sc_basic_authorship::DEFAULT_BLOCK_SIZE_LIMIT.to_string())]
	pub proposer_block_size_limit: usize,

	/// Proposer's soft deadline in percents of block size
	#[clap(long, default_value = "50")]
	pub proposer_soft_deadline_percent: u8,
}
