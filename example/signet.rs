//! Usual sync on Signet.

use kyoto::{builder::NodeBuilder, chain::checkpoints::HeaderCheckpoint};
use kyoto::{AddrV2, Address, Client, Event, Info, Network, ServiceFlags, TrustedPeer};
use std::collections::HashSet;
use std::{
    net::{IpAddr, Ipv4Addr},
    str::FromStr,
};

const NETWORK: Network = Network::Signet;
const RECOVERY_HEIGHT: u32 = 220_000;
const ADDR: &str = "tb1qmfjfv35csd200t0cfpckvx4ccw6w7ytkvga2gn";

#[tokio::main]
async fn main() {
    // Add third-party logging
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    // Use a predefined checkpoint
    let checkpoint = HeaderCheckpoint::closest_checkpoint_below_height(RECOVERY_HEIGHT, NETWORK);
    // Add Bitcoin scripts to scan the blockchain for
    let address = Address::from_str(ADDR)
        .unwrap()
        .require_network(NETWORK)
        .unwrap()
        .into();
    let mut addresses = HashSet::new();
    addresses.insert(address);
    // Add preferred peers to connect to
    let peer_1 = IpAddr::V4(Ipv4Addr::new(95, 217, 198, 121));
    let peer_2 = TrustedPeer::new(
        AddrV2::Ipv4(Ipv4Addr::new(23, 137, 57, 100)),
        None,
        ServiceFlags::P2P_V2,
    );
    // Create a new node builder
    let builder = NodeBuilder::new(NETWORK);
    // Add node preferences and build the node/client
    let (node, client) = builder
        // Add the peers
        .add_peers(vec![(peer_1, None).into(), peer_2])
        // The Bitcoin scripts to monitor
        .add_scripts(addresses)
        // Only scan blocks strictly after a checkpoint
        .after_checkpoint(checkpoint)
        // The number of connections we would like to maintain
        .required_peers(2)
        // Create the node and client
        .build()
        .unwrap();
    // Run the node on a separate task
    tokio::task::spawn(async move { node.run().await });
    // Split the client into components that send messages and listen to messages.
    // With this construction, different parts of the program can take ownership of
    // specific tasks.
    let Client {
        requester,
        mut log_rx,
        mut info_rx,
        mut warn_rx,
        mut event_rx,
    } = client;
    // Continually listen for events until the node is synced to its peers.
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                if let Some(event) = event {
                    match event {
                        Event::Synced(update) => {
                            tracing::info!("Synced chain up to block {}",update.tip().height);
                            tracing::info!("Chain tip: {}",update.tip().hash);
                            // Request information from the node
                            let fee = requester.broadcast_min_feerate().await.unwrap();
                            tracing::info!("Minimum transaction broadcast fee rate: {}", fee);
                            break;
                        },
                        Event::Block(indexed_block) => {
                            let hash = indexed_block.block.block_hash();
                            tracing::info!("Received block: {}", hash);
                        },
                        Event::BlocksDisconnected(_) => {
                            tracing::warn!("Some blocks were reorganized")
                        },
                    }
                }
            }
            info = info_rx.recv() => {
                if let Some(info) = info {
                    match info {
                        Info::StateChange(node_state) => tracing::info!("{node_state}"),
                        Info::ConnectionsMet => tracing::info!("All required connections met"),
                        _ => (),
                    }
                }
            }
            log = log_rx.recv() => {
                if let Some(log) = log {
                    tracing::info!("{log}");
                }
            }
            warn = warn_rx.recv() => {
                if let Some(warn) = warn {
                    tracing::warn!("{warn}");
                }
            }
        }
    }
    let _ = requester.shutdown();
    tracing::info!("Shutting down");
}
