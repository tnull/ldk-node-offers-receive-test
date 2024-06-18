use std::str::FromStr;
use std::sync::Arc;

use tokio::signal::unix::SignalKind;

use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::{Builder, ChannelConfig, Config, Event, LogLevel};

use ldk_node::bitcoin::Network;

#[tokio::main]
async fn main() {
	let args: Vec<String> = std::env::args().collect();

	if args.len() < 5 {
		eprintln!(
			"Usage: {} storage_path listening_addr network esplora_server_url [offer_amount_msat]",
			args[0]
		);
		std::process::exit(-1);
	}

	let mut config = Config::default();
	config.storage_dir_path = args[1].clone();
	config.log_level = LogLevel::Trace;
	config.anchor_channels_config.as_mut().map(|acc| acc.per_channel_reserve_sats = 0);

	config.listening_addresses = match SocketAddress::from_str(&args[2]) {
		Ok(addr) => Some(vec![addr]),
		Err(_) => {
			eprintln!("Failed to parse listening_addr: {}", args[2]);
			std::process::exit(-1);
		},
	};

	config.network = match Network::from_str(&args[3]) {
		Ok(network) => network,
		Err(_) => {
			eprintln!("Unsupported network: {}. Use 'bitcoin', 'testnet', 'regtest', 'signet', 'regtest'.", args[3]);
			std::process::exit(-1);
		},
	};

	let mut builder = Builder::from_config(config.clone());
	builder.set_esplora_server(args[4].clone());

	let offer_amount_msat = if args.len() > 5 {
		match u64::from_str(&args[5]) {
			Ok(amt) => Some(amt),
			Err(_) => {
				eprintln!("Failed to parse amount: {}", args[5]);
				std::process::exit(-1);
			},
		}
	} else {
		None
	};

	let mut sigterm_stream = match tokio::signal::unix::signal(SignalKind::terminate()) {
		Ok(stream) => stream,
		Err(e) => {
			println!("Failed to register for SIGTERM stream: {}", e);
			std::process::exit(-1);
		},
	};

	let node = Arc::new(builder.build().unwrap());
	println!("Starting up...");
	node.start().unwrap();

	println!("CONNECTION_STRING: {}@{}", node.node_id(), config.listening_addresses.as_ref().unwrap().first().unwrap());
	println!("Waiting for an inbound channel...");

	let event_node = Arc::clone(&node);
	loop {
		tokio::select! {
			event = event_node.next_event_async() => {
				match event {
					Event::ChannelPending { channel_id, counterparty_node_id, .. } => {
						println!(
							"CHANNEL_PENDING: {} from counterparty {}",
							channel_id, counterparty_node_id
							);
					},
					Event::ChannelReady { channel_id, counterparty_node_id, user_channel_id, .. } => {
						println!(
							"CHANNEL_READY: {} from counterparty {:?}",
							channel_id, counterparty_node_id
							);

						let channel_config = Arc::new(ChannelConfig::default());
						let max_msats = 21_000_000 * 1_0000_0000 * 1000;
						channel_config.set_max_dust_htlc_exposure_from_fixed_limit(max_msats);
						node.update_channel_config(&user_channel_id, counterparty_node_id.unwrap(), channel_config).unwrap();

						let offer = if let Some(amount_msat) = offer_amount_msat {
							event_node.bolt12_payment().receive(amount_msat, "TEST OFFER").unwrap()
						} else {
							event_node
								.bolt12_payment()
								.receive_variable_amount("VAR-AMT TEST OFFER")
								.unwrap()
						};
						println!("CREATED_OFFER: {}", offer);
					},
					Event::PaymentReceived { payment_id, payment_hash, amount_msat } => {
						println!(
							"PAYMENT_RECEIVED: with id {:?}, hash {}, amount_msat {}",
							payment_id, payment_hash, amount_msat
							);
					},
					_ => {},
				}
				event_node.event_handled();
			},
		_ = tokio::signal::ctrl_c() => {
			println!("Received CTRL-C, shutting down..");
			break;
		}
		_ = sigterm_stream.recv() => {
			println!("Received SIGTERM, shutting down..");
				break;
		}
		}
	}

	std::thread::spawn(move || {
		node.stop().unwrap();
		println!("Shutdown complete..");
	})
	.join()
	.unwrap();
}
