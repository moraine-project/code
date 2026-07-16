use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use reqwest::Url;

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
	let shared = octets[0] == 100 && octets[1] & 0xc0 == 64;
	!(address.is_private()
		|| address.is_loopback()
		|| address.is_link_local()
		|| address.is_unspecified()
		|| address.is_multicast()
		|| address.is_broadcast()
		|| address.is_documentation()
		|| shared)
}

fn is_unique_local(address: Ipv6Addr) -> bool {
	address.segments()[0] & 0xfe00 == 0xfc00
}

pub async fn guard(url: &Url, allow_local: bool) -> Result<(), String> {
	let host = url.host_str().ok_or_else(|| "url has no host".to_string())?;
	let literal_loopback =
		host == "localhost" || host.parse::<IpAddr>().map(|address| address.is_loopback()).unwrap_or(false);
	if allow_local && literal_loopback {
		return Ok(());
	}
	let port = url.port_or_known_default().ok_or_else(|| "url has no port".to_string())?;
	let addresses = tokio::net::lookup_host((host, port))
		.await
		.map_err(|error| format!("could not resolve {host}: {error}"))?;
	let mut resolved = false;
	for address in addresses {
		resolved = true;
		if !is_public_address(address.ip()) {
			return Err(format!("{host} resolves to the non-public address {}", address.ip()));
		}
	}
	if !resolved {
		return Err(format!("{host} did not resolve to any address"));
	}
	Ok(())
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

	#[tokio::test]
	async fn allows_literal_loopback_only_when_permitted() {
		let url = Url::parse("http://127.0.0.1:8098/feed").expect("url");
		assert!(guard(&url, true).await.is_ok());
		assert!(guard(&url, false).await.is_err());
	}

	#[tokio::test]
	async fn rejects_a_private_literal_host() {
		let url = Url::parse("https://169.254.169.254/latest/meta-data").expect("url");
		assert!(guard(&url, false).await.is_err());
	}
}
