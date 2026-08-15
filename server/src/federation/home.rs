use std::net::IpAddr;
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;

use super::FederationError;

pub(crate) struct HomeClient {
	client: reqwest::Client,
	base: Url,
	allow_local: bool,
	max_response_bytes: u64,
}

impl HomeClient {
	pub(crate) fn new(
		base: &str,
		allow_http_local: bool,
		max_response_bytes: u64,
		extra_roots: &[reqwest::Certificate],
	) -> Result<Self, FederationError> {
		let url = validate_home(base, allow_http_local)?;
		let client = crate::federation::egress::client_builder(extra_roots)
			.timeout(Duration::from_secs(10))
			.redirect(reqwest::redirect::Policy::none())
			.build()
			.map_err(|error| FederationError::Http(error.to_string()))?;
		Ok(Self {
			client,
			base: url,
			allow_local: allow_http_local,
			max_response_bytes,
		})
	}

	fn endpoint(&self, path: &str) -> Url {
		let mut url = self.base.clone();
		url.set_path(path.split('?').next().unwrap_or(path));
		if let Some((_, query)) = path.split_once('?') {
			url.set_query(Some(query));
		}
		url
	}

	pub(crate) async fn get_bytes(&self, path: &str) -> Result<Vec<u8>, FederationError> {
		let url = self.endpoint(path);
		crate::federation::egress::guard(&url, self.allow_local)
			.await
			.map_err(FederationError::Http)?;
		let mut response = self
			.client
			.get(url)
			.send()
			.await
			.map_err(|error| FederationError::Http(error.to_string()))?;
		if !response.status().is_success() {
			return Err(FederationError::Http(format!("{} returned {}", path, response.status())));
		}
		if let Some(length) = response.content_length()
			&& length > self.max_response_bytes
		{
			return Err(FederationError::Http(format!("{path} exceeds the response size limit")));
		}
		let mut body = Vec::new();
		while let Some(chunk) = response
			.chunk()
			.await
			.map_err(|error| FederationError::Http(error.to_string()))?
		{
			if body.len() as u64 + chunk.len() as u64 > self.max_response_bytes {
				return Err(FederationError::Http(format!("{path} exceeds the response size limit")));
			}
			body.extend_from_slice(&chunk);
		}
		Ok(body)
	}

	pub(crate) async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, FederationError> {
		let bytes = self.get_bytes(path).await?;
		serde_json::from_slice(&bytes).map_err(|error| FederationError::Decode(error.to_string()))
	}
}

fn validate_home(base: &str, allow_http_local: bool) -> Result<Url, FederationError> {
	let url = Url::parse(base).map_err(|error| FederationError::InvalidUrl(error.to_string()))?;
	match url.scheme() {
		"https" => {}
		"http" => {
			let host = url.host_str().unwrap_or_default().to_string();
			let loopback =
				host == "localhost" || host.parse::<IpAddr>().map(|address| address.is_loopback()).unwrap_or(false);
			if !(allow_http_local && loopback) {
				return Err(FederationError::InvalidUrl(
					"http is only allowed for loopback when explicitly enabled".to_string(),
				));
			}
		}
		_ => return Err(FederationError::InvalidUrl("home url must use https".to_string())),
	}
	if url.host_str().is_none() {
		return Err(FederationError::InvalidUrl("home url has no host".to_string()));
	}
	Ok(url)
}
