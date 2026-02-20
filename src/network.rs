use libp2p::{
	Swarm,
	gossipsub::{self, IdentTopic},
	noise,
	swarm::NetworkBehaviour,
	tcp, yamux,
};

/// `#[derive(NetworkBehaviour)]` generates a `NodeBehaviourEvent::Gossipsub` variant
/// used to pattern-match incoming gossip messages in the network loop.
#[derive(NetworkBehaviour)]
pub struct NodeBehaviour {
	pub gossipsub: gossipsub::Behaviour,
}

pub fn extrinsic_topic() -> IdentTopic {
	IdentTopic::new("extrinsics")
}

pub fn block_topic() -> IdentTopic {
	IdentTopic::new("blocks")
}

pub fn build_swarm() -> Result<Swarm<NodeBehaviour>, Box<dyn std::error::Error>> {
	let swarm = libp2p::SwarmBuilder::with_new_identity()
		.with_tokio()
		.with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
		.with_behaviour(|key| {
			let gossipsub_config = gossipsub::ConfigBuilder::default()
				.heartbeat_interval(std::time::Duration::from_secs(10))
				.validation_mode(gossipsub::ValidationMode::Strict)
				.build()
				.expect("valid gossipsub config");
			let gossipsub = gossipsub::Behaviour::new(
				gossipsub::MessageAuthenticity::Signed(key.clone()),
				gossipsub_config,
			)
			.expect("valid gossipsub behaviour");
			NodeBehaviour { gossipsub }
		})?
		.with_swarm_config(|c| c.with_idle_connection_timeout(std::time::Duration::from_secs(60)))
		.build();
	Ok(swarm)
}
