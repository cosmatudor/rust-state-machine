use std::sync::Arc;

use axum::{
	Router,
	body::Bytes,
	extract::{Path, State},
	http::StatusCode,
	routing::{get, post},
};
use futures::StreamExt;
use libp2p::{Multiaddr, PeerId, gossipsub, swarm::SwarmEvent};
use parity_scale_codec::{Decode, Encode};
use tokio::{
	sync::{Mutex, RwLock, mpsc},
	time,
};

use crate::{network, support, types};

type SharedRuntime = Arc<RwLock<crate::Runtime>>;
type SharedMempool = Arc<Mutex<types::Mempool>>;
/// Sorted by peer ID so every node derives the same authorship sequence independently.
type SharedPeers = Arc<RwLock<Vec<PeerId>>>;

struct PublishReq {
	topic: gossipsub::TopicHash,
	data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Round-robin authorship
// ---------------------------------------------------------------------------

const SLOT_SECS: u64 = 20;

/// Both nodes derive the same slot number from the same wall clock,
/// so no coordination message is needed to agree on the current slot.
fn current_slot() -> u64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_secs() /
		SLOT_SECS
}

async fn is_my_slot(my_id: PeerId, peers: &SharedPeers) -> bool {
	let peers = peers.read().await;
	let idx = (current_slot() as usize) % peers.len();
	peers[idx] == my_id
}

// ---------------------------------------------------------------------------
// RPC server
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RpcState {
	runtime: SharedRuntime,
	mempool: SharedMempool,
	tx_ext: mpsc::UnboundedSender<types::Extrinsic>,
	tx_pub: mpsc::UnboundedSender<PublishReq>,
	ext_topic_hash: gossipsub::TopicHash,
}

/// `POST /submit` — body is a raw SCALE-encoded extrinsic.
async fn submit_handler(
	State(s): State<RpcState>,
	body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
	let raw = body.to_vec();

	let ext = types::Extrinsic::decode(&mut &raw[..])
		.map_err(|e| (StatusCode::BAD_REQUEST, format!("SCALE decode failed: {e}")))?;

	// Gossip to peers so the designated slot author can include the tx even if it wasn't
	// submitted directly to them.
	let _ = s.tx_pub.send(PublishReq { topic: s.ext_topic_hash, data: raw });
	let _ = s.tx_ext.send(ext);

	Ok(StatusCode::ACCEPTED)
}

/// `GET /nonce/<hex_pubkey>` — returns `runtime_nonce + pending_mempool_count`,
/// This lets a client submit multiple txs in rapid succession with correct sequential nonces.
async fn nonce_handler(
	State(s): State<RpcState>,
	Path(hex): Path<String>,
) -> Result<String, (StatusCode, String)> {
	let bytes =
		hex::decode(&hex).map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid hex: {e}")))?;
	if bytes.len() != 32 {
		return Err((StatusCode::BAD_REQUEST, "account must be 32 bytes".into()));
	}
	let mut arr = [0u8; 32];
	arr.copy_from_slice(&bytes);
	let account = crate::support::AccountId32(arr);

	let base = s.runtime.read().await.system.nonce(&account);
	let pending = s
		.mempool
		.lock()
		.await
		.pending_extrinsics()
		.filter(|e| e.signer == account)
		.count() as u32;
	Ok((base + pending).to_string())
}

/// `GET /state` — returns the full runtime debug dump as plain text.
async fn state_handler(State(s): State<RpcState>) -> String {
	let rt = s.runtime.read().await;
	format!("{rt:#?}")
}

async fn start_rpc_server(
	rpc_port: u16,
	runtime: SharedRuntime,
	mempool: SharedMempool,
	tx_ext: mpsc::UnboundedSender<types::Extrinsic>,
	tx_pub: mpsc::UnboundedSender<PublishReq>,
	ext_topic_hash: gossipsub::TopicHash,
) {
	let state = RpcState { runtime, mempool, tx_ext, tx_pub, ext_topic_hash };
	let app = Router::new()
		.route("/submit", post(submit_handler))
		.route("/nonce/:account", get(nonce_handler))
		.route("/state", get(state_handler))
		.with_state(state);

	let addr = format!("0.0.0.0:{rpc_port}");
	let listener = tokio::net::TcpListener::bind(&addr).await.expect("failed to bind RPC port");
	println!("[rpc] listening on http://{addr}");
	axum::serve(listener, app).await.expect("RPC server error");
}

// ---------------------------------------------------------------------------
// P2P node
// ---------------------------------------------------------------------------

pub async fn run(
	port: u16,
	dial_addr: Option<Multiaddr>,
	rpc_port: Option<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
	let runtime: SharedRuntime = {
		let mut rt = crate::Runtime::new();
		crate::maybe_apply_genesis(&mut rt);
		Arc::new(RwLock::new(rt))
	};
	let mempool: SharedMempool = Arc::new(Mutex::new(types::Mempool::with_block_limit(3)));

	let mut swarm = network::build_swarm()?;

	let my_peer_id = swarm.local_peer_id().clone();

	// Peers list is kept sorted at all times so every node derives the same
	// authorship sequence from sorted_peers[slot % len] without any coordination.
	let shared_peers: SharedPeers = Arc::new(RwLock::new(vec![my_peer_id.clone()]));

	let ext_topic = network::extrinsic_topic();
	let blk_topic = network::block_topic();
	swarm.behaviour_mut().gossipsub.subscribe(&ext_topic)?;
	swarm.behaviour_mut().gossipsub.subscribe(&blk_topic)?;

	swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{port}").parse()?)?;
	if let Some(addr) = dial_addr {
		swarm.dial(addr)?;
	}

	let (tx_ext, mut rx_ext) = mpsc::unbounded_channel::<types::Extrinsic>();
	let (tx_blk, mut rx_blk) = mpsc::unbounded_channel::<types::Block>();
	let (tx_pub, mut rx_pub) = mpsc::unbounded_channel::<PublishReq>();

	let ext_hash = ext_topic.hash();
	let blk_hash = blk_topic.hash();

	if let Some(rp) = rpc_port {
		tokio::spawn(start_rpc_server(
			rp,
			Arc::clone(&runtime),
			Arc::clone(&mempool),
			tx_ext.clone(),
			tx_pub.clone(),
			ext_hash.clone(),
		));
	}

	let rt_app = Arc::clone(&runtime);
	let mp_app = Arc::clone(&mempool);
	let tx_pub_app = tx_pub.clone();
	let blk_hash_app = blk_topic.hash();
	let peers_app = Arc::clone(&shared_peers);

	tokio::spawn(async move {
		// Align to the next wall-clock slot boundary so all nodes tick in unison.
		let slot_duration = std::time::Duration::from_secs(SLOT_SECS);
		let now_secs = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_secs();
		let secs_until_next = SLOT_SECS - (now_secs % SLOT_SECS);
		let mut ticker = time::interval_at(
			time::Instant::now() + std::time::Duration::from_secs(secs_until_next),
			slot_duration,
		);

		loop {
			tokio::select! {
				Some(ext) = rx_ext.recv() => {
					// Accumulate in mempool — the slot author seals all pending txs at once.
					let mut pool = mp_app.lock().await;
					let _ = pool.submit(ext);
				}

				Some(block) = rx_blk.recv() => {
					// Snapshot (signer, nonce) pairs before block is moved into execute_block.
					let included: Vec<(support::AccountId32, u32)> =
						block.extrinsics.iter().map(|e| (e.signer, e.nonce)).collect();
					let applied = {
						let mut rt = rt_app.write().await;
						match rt.execute_block(block) {
							Ok(()) => {
								println!("[node] applied peer block, height={}", rt.system.block_number());
								true
							}
							Err(e) => {
								eprintln!("[node] peer block rejected: {e}");
								false
							}
						}
					};
					// Evict the included txs so we don't seal a duplicate block next slot.
					if applied {
						let mut mp = mp_app.lock().await;
						mp.retain(|e| !included.iter().any(|(s, n)| *s == e.signer && *n == e.nonce));
					}
				}

				_ = ticker.tick() => {
					// Don't produce before at least one peer is connected — a lone node
					// advancing the chain would create a fork that peers reject on joining.
					let have_peers = peers_app.read().await.len() > 1;
					if have_peers && is_my_slot(my_peer_id.clone(), &peers_app).await {
						produce_block(
							Arc::clone(&rt_app),
							Arc::clone(&mp_app),
							tx_pub_app.clone(),
							blk_hash_app.clone(),
						).await;
					}
				}
			}
		}
	});

	loop {
		tokio::select! {
			event = swarm.select_next_some() => {
				match event {
					SwarmEvent::NewListenAddr { address, .. } => {
						println!("[net] listening on {address}");
					}
					SwarmEvent::Behaviour(network::NodeBehaviourEvent::Gossipsub(
						gossipsub::Event::Message { message, .. },
					)) => {
						if message.topic == ext_hash {
							match types::Extrinsic::decode(&mut &message.data[..]) {
								Ok(ext) => { let _ = tx_ext.send(ext); }
								Err(e) => eprintln!("[net] bad extrinsic bytes: {e}"),
							}
						} else if message.topic == blk_hash {
							match types::Block::decode(&mut &message.data[..]) {
								Ok(blk) => { let _ = tx_blk.send(blk); }
								Err(e) => eprintln!("[net] bad block bytes: {e}"),
							}
						}
					}
					SwarmEvent::ConnectionEstablished { peer_id, .. } => {
						println!("[net] connected to {peer_id}");
						let mut peers = shared_peers.write().await;
						peers.push(peer_id);
						peers.sort();
						println!("[node] author order: {:?}", peers.iter().map(|p| p.to_base58()[..8].to_string()).collect::<Vec<_>>());
					}
					SwarmEvent::ConnectionClosed { peer_id, .. } => {
						println!("[net] disconnected {peer_id}");
						let mut peers = shared_peers.write().await;
						peers.retain(|id| *id != peer_id);
					}
					_ => {}
				}
			}

			Some(req) = rx_pub.recv() => {
				if let Err(e) = swarm.behaviour_mut().gossipsub.publish(req.topic, req.data) {
					eprintln!("[net] publish error: {e:?}");
				}
			}
		}
	}
}

// ---------------------------------------------------------------------------
// Block production
// ---------------------------------------------------------------------------

async fn produce_block(
	runtime: SharedRuntime,
	mempool: SharedMempool,
	tx_pub: mpsc::UnboundedSender<PublishReq>,
	blk_topic: gossipsub::TopicHash,
) {
	let candidates = {
		let mut mp = mempool.lock().await;
		let limit = mp.block_limit().unwrap_or(10);
		mp.drain_for_block(limit)
	};

	// Group by signer and include only consecutive nonces starting from the current runtime
	// nonce. Multiple txs from the same account land in one block; stale nonces are dropped.
	let batch: Vec<_> = {
		let rt = runtime.read().await;
		let mut by_signer: std::collections::HashMap<support::AccountId32, Vec<_>> =
			std::collections::HashMap::new();
		for ext in candidates {
			by_signer.entry(ext.signer).or_default().push(ext);
		}
		let mut result = Vec::new();
		for (signer, mut txs) in by_signer {
			txs.sort_by_key(|e| e.nonce);
			let mut expected = rt.system.nonce(&signer);
			for tx in txs {
				if tx.nonce == expected {
					expected += 1;
					result.push(tx);
				} else {
					break; // gap — higher nonces can't be applied without the missing one
				}
			}
		}
		result
	};

	let mut rt = runtime.write().await;
	let next_num = rt.system.block_number().checked_add(1u32).unwrap();
	let block =
		types::Block { header: support::Header { block_number: next_num }, extrinsics: batch };

	let encoded = block.encode();
	let tx_summary: Vec<String> = block
		.extrinsics
		.iter()
		.map(|e| format!("    signer={:?} nonce={}", e.signer, e.nonce))
		.collect();
	match rt.execute_block(block) {
		Ok(()) => {
			println!("[node] produced block #{next_num} ({} tx)", tx_summary.len());
			for line in &tx_summary {
				println!("{line}");
			}
			let _ = tx_pub.send(PublishReq { topic: blk_topic, data: encoded });
		},
		Err(e) => eprintln!("[node] block production failed: {e}"),
	}
}
