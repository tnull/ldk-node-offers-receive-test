use std::io;
use std::io::prelude::*;
use std::str::FromStr;
use std::sync::Arc;

use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::{Builder, Config, Event, LogLevel};

use ldk_node::bitcoin::Network;

fn main() {
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
	config.anchor_channels_config = None;

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

	let mut builder = Builder::from_config(config);
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

	let node = Arc::new(builder.build().unwrap());
	node.start().unwrap();

	println!("NODE_ID: {}", node.node_id());

	let event_node = Arc::clone(&node);
	std::thread::spawn(move || loop {
		let event = event_node.wait_next_event();
		match event {
			Event::ChannelPending { channel_id, counterparty_node_id, .. } => {
				println!(
					"CHANNEL_PENDING: {} from counterparty {}",
					channel_id, counterparty_node_id
				);
			},
			Event::ChannelReady { channel_id, counterparty_node_id, .. } => {
				println!(
					"CHANNEL_READY: {} from counterparty {:?}",
					channel_id, counterparty_node_id
				);

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
	});

	pause();

	node.stop().unwrap();
}

fn pause() {
	let mut stdin = io::stdin();
	let mut stdout = io::stdout();

	// We want the cursor to stay at the end of the line, so we print without a newline and flush manually.
	write!(stdout, "Press any key to continue...").unwrap();
	stdout.flush().unwrap();

	// Read a single byte and discard
	let _ = stdin.read(&mut [0u8]).unwrap();
}
