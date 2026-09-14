use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub fn client_builder(extra_roots: &[reqwest::Certificate]) -> reqwest::ClientBuilder {
	let mut builder = reqwest::Client::builder();
	for root in extra_roots {
		builder = builder.add_root_certificate(root.clone());
	}
	builder
}

pub fn load_extra_roots(path: &std::path::Path) -> (Vec<reqwest::Certificate>, usize) {
	let text = match std::fs::read_to_string(path) {
		Ok(text) => text,
		Err(error) => {
			tracing::warn!(path = %path.display(), %error, "extra TLS roots were not read");
			return (Vec::new(), 0);
		}
	};
	let blocks = pem_blocks(&text);
	let total = blocks.len();
	let roots: Vec<reqwest::Certificate> = blocks
		.iter()
		.filter_map(|block| reqwest::Certificate::from_pem(block.as_bytes()).ok())
		.collect();
	let skipped = total.saturating_sub(roots.len());
	(roots, skipped)
}

fn pem_blocks(text: &str) -> Vec<String> {
	const END: &str = "-----END CERTIFICATE-----";
	let mut blocks = Vec::new();
	let mut current = String::new();
	let mut inside = false;
	for line in text.lines() {
		if line.contains("-----BEGIN CERTIFICATE-----") {
			inside = true;
			current.clear();
		}
		if inside {
			current.push_str(line);
			current.push('\n');
		}
		if inside && line.contains(END) {
			blocks.push(current.clone());
			inside = false;
		}
	}
	blocks
}

pub fn is_public_address(address: IpAddr) -> bool {
	match address {
		IpAddr::V4(address) => is_public_v4(address),
		IpAddr::V6(address) => {
			if let Some(mapped) = address.to_ipv4_mapped() {
				return is_public_v4(mapped);
			}
			!(address.is_loopback()
				|| address.is_unspecified()
				|| address.is_multicast()
				|| address.is_unicast_link_local()
				|| is_unique_local(address))
		}
	}
}

fn is_public_v4(address: Ipv4Addr) -> bool {
	let octets = address.octets();
	let [first, second, ..] = octets;
	let shared = first == 100 && second & 0xc0 == 64;
	let this_network = first == 0;
	let benchmarking = first == 198 && (second == 18 || second == 19);
	let reserved = first >= 240;
	!(address.is_private()
		|| address.is_loopback()
		|| address.is_link_local()
		|| address.is_unspecified()
		|| address.is_multicast()
		|| address.is_broadcast()
		|| address.is_documentation()
		|| shared
		|| this_network
		|| benchmarking
		|| reserved)
}

fn is_unique_local(address: Ipv6Addr) -> bool {
	address.segments()[0] & 0xfe00 == 0xfc00
}

pub async fn resolve_public(host: &str, port: u16, allow_local: bool) -> Result<Vec<IpAddr>, String> {
	if let Ok(literal) = host.parse::<IpAddr>() {
		if !is_public_address(literal) && !(allow_local && literal.is_loopback()) {
			return Err(format!("{host} is not a public address"));
		}
		return Ok(vec![literal]);
	}
	let addresses = tokio::net::lookup_host((host, port))
		.await
		.map_err(|error| format!("could not resolve {host}: {error}"))?;
	let mut resolved = Vec::new();
	for address in addresses {
		let ip = address.ip();
		if !is_public_address(ip) && !(allow_local && ip.is_loopback()) {
			return Err(format!("{host} resolves to the non-public address {ip}"));
		}
		resolved.push(ip);
	}
	if resolved.is_empty() {
		return Err(format!("{host} did not resolve to any address"));
	}
	Ok(resolved)
}

pub fn pinned(builder: reqwest::ClientBuilder, host: &str, port: u16, addresses: &[IpAddr]) -> reqwest::ClientBuilder {
	if host.parse::<IpAddr>().is_ok() {
		return builder;
	}
	let addresses: Vec<SocketAddr> = addresses.iter().map(|ip| SocketAddr::new(*ip, port)).collect();
	builder.resolve_to_addrs(host, &addresses)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rejects_internal_and_metadata_addresses() {
		for address in [
			"127.0.0.1",
			"10.0.0.1",
			"172.16.5.4",
			"192.168.1.1",
			"169.254.169.254",
			"100.64.0.1",
			"0.0.0.0",
			"::1",
			"fe80::1",
			"fd00::1",
			"::ffff:10.0.0.1",
		] {
			assert!(
				!is_public_address(address.parse().expect("address")),
				"{address} should be rejected"
			);
		}
	}

	#[test]
	fn accepts_routable_addresses() {
		for address in ["1.1.1.1", "93.184.216.34", "2606:4700:4700::1111", "2001:db8::1"] {
			assert!(
				is_public_address(address.parse().expect("address")),
				"{address} should be accepted"
			);
		}
	}

	#[test]
	fn splits_pem_bundles() {
		let text = "-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\nnoise\n-----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----\n";
		let blocks = pem_blocks(text);
		assert_eq!(blocks.len(), 2);
		assert!(blocks[0].contains("AAA"));
		assert!(blocks[1].contains("BBB"));
	}

	#[tokio::test]
	async fn allows_literal_loopback_only_when_permitted() {
		assert!(resolve_public("127.0.0.1", 8098, true).await.is_ok());
		assert!(resolve_public("127.0.0.1", 8098, false).await.is_err());
	}

	#[tokio::test]
	async fn resolves_and_pins_a_name_to_its_validated_address() {
		let addresses = resolve_public("localhost", 8098, true).await.expect("loopback");
		assert!(addresses.iter().all(|address| address.is_loopback()));
		let builder = pinned(client_builder(&[]), "localhost", 8098, &addresses);
		builder.build().expect("pinned client");
	}

	#[tokio::test]
	async fn rejects_a_private_literal_host() {
		assert!(resolve_public("169.254.169.254", 443, false).await.is_err());
	}
}
