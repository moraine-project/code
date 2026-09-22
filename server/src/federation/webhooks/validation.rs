use reqwest::Url;

pub(super) fn validate_url(value: &str, allow_http_local: bool) -> Result<Url, ()> {
	let url = Url::parse(value).map_err(|_| ())?;
	match url.scheme() {
		"https" => {}
		"http" => {
			let host = url.host_str().unwrap_or_default().to_string();
			let loopback =
				host == "localhost" || host.parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false);
			if !(allow_http_local && loopback) {
				return Err(());
			}
		}
		_ => return Err(()),
	}
	if url.host_str().is_none() {
		return Err(());
	}
	Ok(url)
}
