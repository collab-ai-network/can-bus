//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.
// TODO:: move to core_primitives
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature,
};
use sp_runtime::traits::BlakeTwo256;
/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
/// Balance of an account.
pub type Balance = u128;
/// Index of a transaction in the chain.
pub type Nonce = u32;
// TODO:: move to core_primitives
use futures::FutureExt;
use node_template_runtime::{self, opaque::Block, RuntimeApi};
use sc_client_api::{Backend, BlockBackend};
use sc_consensus_aura::{ImportQueueParams, SlotProportion, StartAuraParams};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sc_consensus_grandpa::SharedVoterState;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager, WarpSyncParams};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_api::ConstructRuntimeApi;
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use std::{sync::Arc, time::Duration};

pub(crate) type FullClient = sc_service::TFullClient<
	Block,
	RuntimeApi,
	sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

#[cfg(not(feature = "runtime-benchmarks"))]
type HostFunctions =
	(sp_io::SubstrateHostFunctions, moonbeam_primitives_ext::moonbeam_ext::HostFunctions);

#[cfg(feature = "runtime-benchmarks")]
type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
	moonbeam_primitives_ext::moonbeam_ext::HostFunctions,
);

// Native executor instance.
pub struct CollabAIRuntimeExecutor;
impl sc_executor::NativeExecutionDispatch for CollabAIRuntimeExecutor {
	type ExtendHostFunctions = HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		node_template_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		node_template_runtime::native_version()
	}
}

/// The minimum period of blocks on which justifications will be
/// imported and generated.
const GRANDPA_JUSTIFICATION_PERIOD: u32 = 512;

pub type Service = sc_service::PartialComponents<
	FullClient,
	FullBackend,
	FullSelectChain,
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::FullPool<Block, FullClient>,
	(
		sc_consensus_grandpa::GrandpaBlockImport<FullBackend, Block, FullClient, FullSelectChain>,
		sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
		Option<Telemetry>,
	),
>;

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial<RuntimeApi, Executor, BIQ>(
	config: &Configuration,
	build_import_queue: BIQ,
) -> Result<
	PartialComponents<
		FullClient<RuntimeApi, Executor>,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block, FullClient<RuntimeApi, Executor>>,
		sc_transaction_pool::FullPool<Block, FullClient<RuntimeApi, Executor>>,
		(
			ParachainBlockImport<RuntimeApi, Executor>,
			Option<Telemetry>,
			Option<TelemetryWorkerHandle>,
			Arc<fc_db::kv::Backend<Block>>,
		),
	>,
	sc_service::Error,
>
where
	RuntimeApi:
		ConstructRuntimeApi<Block, FullClient<RuntimeApi, Executor>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<
			Block,
			StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>,
		> + sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ fp_rpc::EthereumRuntimeRPCApi<Block>,
	sc_client_api::StateBackendFor<FullBackend, Block>: sp_api::StateBackend<BlakeTwo256>,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
	BIQ: FnOnce(
		Arc<FullClient<RuntimeApi, Executor>>,
		ParachainBlockImport<RuntimeApi, Executor>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
		bool,
	) -> Result<
		sc_consensus::DefaultImportQueue<Block, FullClient<RuntimeApi, Executor>>,
		sc_service::Error,
	>,
{
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor = WasmExecutor::builder()
		.with_execution_method(config.wasm_method)
		.with_onchain_heap_alloc_strategy(
			config.default_heap_pages.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| {
				HeapAllocStrategy::Static { extra_pages: h as _ }
			}),
		)
		.with_offchain_heap_alloc_strategy(
			config.default_heap_pages.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| {
				HeapAllocStrategy::Static { extra_pages: h as _ }
			}),
		)
		.with_max_runtime_instances(config.max_runtime_instances)
		.with_runtime_cache_size(config.runtime_cache_size)
		.build();

	let executor =
		sc_executor::NativeElseWasmExecutor::<Executor>::new_with_wasm_executor(executor);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let select_chain = Some(LongestChain::new(backend.clone()));

	let frontier_backend = rpc::open_frontier_backend(client.clone(), config)?;

	let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let import_queue = build_import_queue(
		client.clone(),
		block_import.clone(),
		config,
		telemetry.as_ref().map(|telemetry| telemetry.handle()),
		&task_manager,
	)?;

	let params = PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain,
		other: (block_import, telemetry, telemetry_worker_handle, frontier_backend),
	};

	Ok(params)
}

/// Builds a new service for a full client.
// start a standalone node which doesn't need to connect to relaychain
pub async fn new_full<RuntimeApi, Executor>(
	config: Configuration,
	evm_tracing_config: crate::evm_tracing_types::EvmTracingConfig,
) -> Result<TaskManager, sc_service::Error>
where
	RuntimeApi:
		ConstructRuntimeApi<Block, FullClient<RuntimeApi, Executor>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<
			Block,
			StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>,
		> + sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ sp_consensus_aura::AuraApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ moonbeam_rpc_primitives_debug::DebugRuntimeApi<Block>
		+ moonbeam_rpc_primitives_txpool::TxPoolRuntimeApi<Block>
		+ fp_rpc::EthereumRuntimeRPCApi<Block>
		+ fp_rpc::ConvertTransactionRuntimeApi<Block>,
	sc_client_api::StateBackendFor<FullBackend, Block>: sp_api::StateBackend<BlakeTwo256>,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain: maybe_select_chain,
		transaction_pool,
		other: (_, _, _, frontier_backend),
	} = new_partial::<RuntimeApi, Executor, _>(&config, build_import_queue::<RuntimeApi, Executor>)?;

	// Sinks for pubsub notifications.
	// Everytime a new subscription is created, a new mpsc channel is added to the sink pool.
	// The MappingSyncWorker sends through the channel on block import and the subscription
	// emits a notification to the subscriber on receiving a message through this channel. This
	// way we avoid race conditions when using native substrate block import notification
	// stream.
	let pubsub_notification_sinks: fc_mapping_sync::EthereumBlockNotificationSinks<
		fc_mapping_sync::EthereumBlockNotification<Block>,
	> = Default::default();
	let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

	let net_config = sc_network::config::FullNetworkConfiguration::new(&config.network);

	let (network, system_rpc_tx, tx_handler_controller, start_network, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_params: None,
			block_relay: None,
		})?;

	let role = config.role.clone();
	let force_authoring = config.force_authoring;
	let backoff_authoring_blocks: Option<()> = None;

	let (
		filter_pool,
		fee_history_limit,
		fee_history_cache,
		block_data_cache,
		overrides,
		tracing_requesters,
		ethapi_cmd,
	) = start_node_evm_impl::<RuntimeApi, Executor>(
		client.clone(),
		backend.clone(),
		frontier_backend.clone(),
		&mut task_manager,
		&config,
		evm_tracing_config.clone(),
		sync_service.clone(),
		pubsub_notification_sinks.clone(),
	);

	let select_chain = maybe_select_chain
		.expect("In `standalone` mode, `new_partial` will return some `select_chain`; qed");

	if role.is_authority() {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			None,
			None,
		);
		// aura
		let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
		let client_for_cidp = client.clone();

		let aura = sc_consensus_aura::start_aura::<
			sp_consensus_aura::sr25519::AuthorityPair,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
		>(StartAuraParams {
			slot_duration: sc_consensus_aura::slot_duration(&*client)?,
			client: client.clone(),
			select_chain,
			block_import: StandaloneBlockImport::new(client.clone()),
			proposer_factory,
			create_inherent_data_providers: move |block: Hash, ()| {
				let current_para_block = client_for_cidp
					.number(block)
					.expect("Header lookup should succeed")
					.expect("Header passed in as parent should be present in backend.");
				let client_for_xcm = client_for_cidp.clone();

				async move {
					let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

					let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
						*timestamp,
						slot_duration,
					);

					let mocked_parachain = MockValidationDataInherentDataProvider {
						current_para_block,
						relay_offset: 1000,
						relay_blocks_per_para_block: 2,
						para_blocks_per_relay_epoch: 0,
						relay_randomness_config: (),
						xcm_config: MockXcmConfig::new(
							&*client_for_xcm,
							block,
							Default::default(),
							Default::default(),
						),
						raw_downward_messages: vec![],
						raw_horizontal_messages: vec![],
					};

					Ok((slot, timestamp, mocked_parachain))
				}
			},
			force_authoring,
			backoff_authoring_blocks,
			keystore: keystore_container.keystore(),
			sync_oracle: sync_service.clone(),
			justification_sync_link: sync_service.clone(),
			// We got around 500ms for proposing
			block_proposal_slot_portion: SlotProportion::new(1f32 / 24f32),
			// And a maximum of 750ms if slots are skipped
			max_block_proposal_slot_portion: Some(SlotProportion::new(1f32 / 16f32)),
			telemetry: None,
			compatibility_mode: Default::default(),
		})?;

		// the AURA authoring task is considered essential, i.e. if it
		// fails we take down the service with it.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("aura", Some("block-authoring"), aura);
	}

	let rpc_builder = {
		let client = client.clone();
		let network = network.clone();
		let transaction_pool = transaction_pool.clone();
		let rpc_config = rpc::EvmTracingConfig {
			tracing_requesters,
			trace_filter_max_count: evm_tracing_config.ethapi_trace_max_count,
			enable_txpool: ethapi_cmd.contains(&EthApiCmd::TxPool),
		};
		let sync = sync_service.clone();
		let pubsub_notification_sinks = pubsub_notification_sinks;

		Box::new(move |deny_unsafe, subscription| {
			let deps = rpc::FullDeps {
				client: client.clone(),
				pool: transaction_pool.clone(),
				graph: transaction_pool.pool().clone(),
				network: network.clone(),
				sync: sync.clone(),
				is_authority: role.is_authority(),
				deny_unsafe,
				frontier_backend: frontier_backend.clone(),
				filter_pool: filter_pool.clone(),
				fee_history_limit,
				fee_history_cache: fee_history_cache.clone(),
				block_data_cache: block_data_cache.clone(),
				overrides: overrides.clone(),
				// enable EVM RPC for dev node by default
				enable_evm_rpc: true,
			};

			crate::rpc::create_full(
				deps,
				subscription,
				pubsub_notification_sinks.clone(),
				rpc_config.clone(),
			)
			.map_err(Into::into)
		})
	};

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		rpc_builder,
		client,
		transaction_pool,
		task_manager: &mut task_manager,
		config,
		keystore: keystore_container.keystore(),
		backend,
		network,
		system_rpc_tx,
		sync_service,
		tx_handler_controller,
		telemetry: None,
	})?;

	start_network.start_network();

	Ok(task_manager)
}

pub fn start_node_evm_impl<RuntimeApi, Executor>(
	client: Arc<FullClient<RuntimeApi, Executor>>,
	backend: Arc<FullBackend>,
	frontier_backend: Arc<fc_db::kv::Backend<Block>>,
	task_manager: &mut TaskManager,
	config: &Configuration,
	evm_tracing_config: crate::evm_tracing_types::EvmTracingConfig,
	sync_service: Arc<SyncingService<Block>>,
	pubsub_notification_sinks: Arc<
		fc_mapping_sync::EthereumBlockNotificationSinks<
			fc_mapping_sync::EthereumBlockNotification<Block>,
		>,
	>,
) -> (
	FilterPool,
	u64,
	FeeHistoryCache,
	Arc<EthBlockDataCacheTask<Block>>,
	Arc<OverrideHandle<Block>>,
	RpcRequesters,
	Vec<EthApiCmd>,
)
where
	RuntimeApi:
		ConstructRuntimeApi<Block, FullClient<RuntimeApi, Executor>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<
			Block,
			StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>,
		> + sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ moonbeam_rpc_primitives_debug::DebugRuntimeApi<Block>
		+ moonbeam_rpc_primitives_txpool::TxPoolRuntimeApi<Block>
		+ fp_rpc::EthereumRuntimeRPCApi<Block>
		+ fp_rpc::ConvertTransactionRuntimeApi<Block>,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let prometheus_registry = config.prometheus_registry().cloned();
	let filter_pool: FilterPool = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
	let fee_history_cache: FeeHistoryCache = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
	let overrides = fc_storage::overrides_handle(client.clone());

	let ethapi_cmd = evm_tracing_config.ethapi.clone();
	let tracing_requesters =
		if ethapi_cmd.contains(&EthApiCmd::Debug) || ethapi_cmd.contains(&EthApiCmd::Trace) {
			tracing::spawn_tracing_tasks(
				&evm_tracing_config,
				prometheus_registry.clone(),
				tracing::SpawnTasksParams {
					task_manager,
					client: client.clone(),
					substrate_backend: backend.clone(),
					frontier_backend: frontier_backend.clone(),
					filter_pool: Some(filter_pool.clone()),
					overrides: overrides.clone(),
				},
			)
		} else {
			tracing::RpcRequesters { debug: None, trace: None }
		};

	// Frontier offchain DB task. Essential.
	// Maps emulated ethereum data to substrate native data.
	task_manager.spawn_essential_handle().spawn(
		"frontier-mapping-sync-worker",
		Some("frontier"),
		fc_mapping_sync::kv::MappingSyncWorker::new(
			client.import_notification_stream(),
			Duration::new(6, 0),
			client.clone(),
			backend,
			overrides.clone(),
			frontier_backend,
			3,
			0,
			fc_mapping_sync::SyncStrategy::Parachain,
			sync_service,
			pubsub_notification_sinks,
		)
		.for_each(|()| futures::future::ready(())),
	);

	// Frontier `EthFilterApi` maintenance. Manages the pool of user-created Filters.
	// Each filter is allowed to stay in the pool for 100 blocks.
	const FILTER_RETAIN_THRESHOLD: u64 = 100;
	task_manager.spawn_essential_handle().spawn(
		"frontier-filter-pool",
		Some("frontier"),
		fc_rpc::EthTask::filter_pool_task(
			client.clone(),
			filter_pool.clone(),
			FILTER_RETAIN_THRESHOLD,
		),
	);

	const FEE_HISTORY_LIMIT: u64 = 2048;
	task_manager.spawn_essential_handle().spawn(
		"frontier-fee-history",
		Some("frontier"),
		fc_rpc::EthTask::fee_history_task(
			client,
			overrides.clone(),
			fee_history_cache.clone(),
			FEE_HISTORY_LIMIT,
		),
	);

	let block_data_cache = Arc::new(fc_rpc::EthBlockDataCacheTask::new(
		task_manager.spawn_handle(),
		overrides.clone(),
		50,
		50,
		prometheus_registry,
	));

	(
		filter_pool,
		FEE_HISTORY_LIMIT,
		fee_history_cache,
		block_data_cache,
		overrides,
		tracing_requesters,
		ethapi_cmd,
	)
}

/// Build the import queue for the litmus/litentry/rococo runtime.
pub fn build_import_queue<RuntimeApi, Executor>(
	client: Arc<FullClient<RuntimeApi, Executor>>,
	block_import: ParachainBlockImport<RuntimeApi, Executor>,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<
	sc_consensus::DefaultImportQueue<Block, FullClient<RuntimeApi, Executor>>,
	sc_service::Error,
>
where
	RuntimeApi:
		ConstructRuntimeApi<Block, FullClient<RuntimeApi, Executor>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<
			Block,
			StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>,
		> + sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ sp_consensus_aura::AuraApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ fp_rpc::EthereumRuntimeRPCApi<Block>,
	sc_client_api::StateBackendFor<FullBackend, Block>: sp_api::StateBackend<BlakeTwo256>,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	// aura import queue
	let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
	let client_for_cidp = client.clone();

	sc_consensus_aura::import_queue::<sp_consensus_aura::sr25519::AuthorityPair, _, _, _, _, _>(
		sc_consensus_aura::ImportQueueParams {
			block_import,
			justification_import: None,
			client,
			create_inherent_data_providers: move |block: Hash, ()| {
				let current_para_block = client_for_cidp
					.number(block)
					.expect("Header lookup should succeed")
					.expect("Header passed in as parent should be present in backend.");
				let client_for_xcm = client_for_cidp.clone();

				async move {
					let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

					let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
							*timestamp,
							slot_duration,
						);

					let mocked_parachain = MockValidationDataInherentDataProvider {
						current_para_block,
						relay_offset: 1000,
						relay_blocks_per_para_block: 2,
						para_blocks_per_relay_epoch: 0,
						relay_randomness_config: (),
						xcm_config: MockXcmConfig::new(
							&*client_for_xcm,
							block,
							Default::default(),
							Default::default(),
						),
						raw_downward_messages: vec![],
						raw_horizontal_messages: vec![],
					};

					Ok((slot, timestamp, mocked_parachain))
				}
			},
			spawner: &task_manager.spawn_essential_handle(),
			registry: config.prometheus_registry(),
			check_for_equivocation: Default::default(),
			telemetry,
			compatibility_mode: Default::default(),
		},
	)
	.map_err(Into::into)
}