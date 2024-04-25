use crate::{
	benchmarking::{inherent_benchmark_data, RemarkBuilder, TransferKeepAliveBuilder},
	chain_spec,
	cli::{Cli, Subcommand},
	service::*,
};
use frame_benchmarking_cli::{BenchmarkCmd, ExtrinsicFactory, SUBSTRATE_REFERENCE_HARDWARE};
use node_template_runtime::{Block, EXISTENTIAL_DEPOSIT};
use sc_cli::SubstrateCli;
use sc_service::PartialComponents;
use sp_keyring::Sr25519Keyring;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"CollabAI Node".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		"CollabAI node\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relay chain node.\n\n\
		litentry-collator <parachain-args> -- <relay-chain-args>"
			.into()
			.into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/collab-ai-network/can-bus/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(match id {
			"dev" => Box::new(chain_spec::development_config()?),
			"" | "local" => Box::new(chain_spec::local_testnet_config()?),
			path =>
				Box::new(chain_spec::ChainSpec::from_json_file(std::path::PathBuf::from(path))?),
		})
	}
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Key(cmd)) => cmd.run(&cli),
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					new_partial::<
						node_template_runtime::RuntimeApi,
						CollabAIRuntimeExecutor,
						_,
					>(
						&config,
						build_import_queue::<
							node_template_runtime::RuntimeApi,
							CollabAIRuntimeExecutor,
						>,
					)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, .. } = new_partial::<
					node_template_runtime::RuntimeApi,
					CollabAIRuntimeExecutor,
					_,
				>(
					&config,
					build_import_queue::<
						node_template_runtime::RuntimeApi,
						CollabAIRuntimeExecutor,
					>,
				)?;
				Ok((cmd.run(client, config.database), task_manager))
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, .. } = new_partial::<
					node_template_runtime::RuntimeApi,
					CollabAIRuntimeExecutor,
					_,
				>(
					&config,
					build_import_queue::<
						node_template_runtime::RuntimeApi,
						CollabAIRuntimeExecutor,
					>,
				)?;
				Ok((cmd.run(client, config.chain_spec), task_manager))
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					new_partial::<
						node_template_runtime::RuntimeApi,
						CollabAIRuntimeExecutor,
						_,
					>(
						&config,
						build_import_queue::<
							node_template_runtime::RuntimeApi,
							CollabAIRuntimeExecutor,
						>,
					)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.database))
		},
		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, backend, .. } = new_partial::<
					node_template_runtime::RuntimeApi,
					CollabAIRuntimeExecutor,
					_,
				>(
					&config,
					build_import_queue::<
						node_template_runtime::RuntimeApi,
						CollabAIRuntimeExecutor,
					>,
				)?;
				let aux_revert = Box::new(|client, _, blocks| {
					sc_consensus_grandpa::revert(client, blocks)?;
					Ok(())
				});
				Ok((cmd.run(client, backend, Some(aux_revert)), task_manager))
			})
		},
		Some(Subcommand::Benchmark(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				// This switch needs to be in the client, since the client decides
				// which sub-commands it wants to support.
				match cmd {
					BenchmarkCmd::Pallet(cmd) => {
						if !cfg!(feature = "runtime-benchmarks") {
							return Err(
								"Runtime benchmarking wasn't enabled when building the node. \
							You can enable it with `--features runtime-benchmarks`."
									.into(),
							);
						}

						cmd.run::<sp_runtime::traits::HashingFor<Block>, ()>(config)
					},
					BenchmarkCmd::Block(cmd) => {
						let PartialComponents { client, .. } = new_partial::<
							node_template_runtime::RuntimeApi,
							CollabAIRuntimeExecutor,
							_,
						>(
							&config,
							build_import_queue::<
								node_template_runtime::RuntimeApi,
								CollabAIRuntimeExecutor,
							>,
						)?;
						cmd.run(client)
					},
					#[cfg(not(feature = "runtime-benchmarks"))]
					BenchmarkCmd::Storage(_) => Err(
						"Storage benchmarking can be enabled with `--features runtime-benchmarks`."
							.into(),
					),
					#[cfg(feature = "runtime-benchmarks")]
					BenchmarkCmd::Storage(cmd) => {
						let PartialComponents { client, backend, .. } = new_partial::<
							node_template_runtime::RuntimeApi,
							CollabAIRuntimeExecutor,
							_,
						>(
							&config,
							build_import_queue::<
								node_template_runtime::RuntimeApi,
								CollabAIRuntimeExecutor,
							>,
						)?;
						let db = backend.expose_db();
						let storage = backend.expose_storage();

						cmd.run(config, client, db, storage)
					},
					BenchmarkCmd::Overhead(cmd) => {
						let PartialComponents { client, .. } = new_partial::<
							node_template_runtime::RuntimeApi,
							CollabAIRuntimeExecutor,
							_,
						>(
							&config,
							build_import_queue::<
								node_template_runtime::RuntimeApi,
								CollabAIRuntimeExecutor,
							>,
						)?;
						let ext_builder = RemarkBuilder::new(client.clone());

						cmd.run(
							config,
							client,
							inherent_benchmark_data()?,
							Vec::new(),
							&ext_builder,
						)
					},
					BenchmarkCmd::Extrinsic(cmd) => {
						let PartialComponents { client, .. } = new_partial::<
							node_template_runtime::RuntimeApi,
							CollabAIRuntimeExecutor,
							_,
						>(
							&config,
							build_import_queue::<
								node_template_runtime::RuntimeApi,
								CollabAIRuntimeExecutor,
							>,
						)?;
						// Register the *Remark* and *TKA* builders.
						let ext_factory = ExtrinsicFactory(vec![
							Box::new(RemarkBuilder::new(client.clone())),
							Box::new(TransferKeepAliveBuilder::new(
								client.clone(),
								Sr25519Keyring::Alice.to_account_id(),
								EXISTENTIAL_DEPOSIT,
							)),
						]);

						cmd.run(client, inherent_benchmark_data()?, Vec::new(), &ext_factory)
					},
					BenchmarkCmd::Machine(cmd) =>
						cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone()),
				}
			})
		},
		#[cfg(feature = "try-runtime")]
		Some(Subcommand::TryRuntime) => Err(try_runtime_cli::DEPRECATION_NOTICE.into()),
		#[cfg(not(feature = "try-runtime"))]
		Some(Subcommand::TryRuntime) => Err("TryRuntime wasn't enabled when building the node. \
				You can enable it with `--features try-runtime`."
			.into()),
		Some(Subcommand::ChainInfo(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run::<Block>(&config))
		},
		None => {
			let runner = cli.create_runner(&cli.run)?;

			let evm_tracing_config = crate::evm_tracing_types::EvmTracingConfig {
				ethapi: cli.eth_api_options.ethapi,
				ethapi_max_permits: cli.eth_api_options.ethapi_max_permits,
				ethapi_trace_max_count: cli.eth_api_options.ethapi_trace_max_count,
				ethapi_trace_cache_duration: cli.eth_api_options.ethapi_trace_cache_duration,
				eth_log_block_cache: cli.eth_api_options.eth_log_block_cache,
				eth_statuses_cache: cli.eth_api_options.eth_statuses_cache,
				max_past_logs: cli.eth_api_options.max_past_logs,
				tracing_raw_max_memory_usage: cli.eth_api_options.tracing_raw_max_memory_usage,
			};
			runner.run_node_until_exit(|config| async move {
				new_full(config, evm_tracing_config).map_err(sc_cli::Error::Service)
			})
		},
	}
}
