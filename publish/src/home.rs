use std::path::Path;
use std::time::Duration;

use reqwest::Url;

fn extra_roots(path: &Path) -> Result<Vec<reqwest::Certificate>, String> {
	let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
	let blocks = pem_blocks(&text);
	if blocks.is_empty() {
		return Err(format!("{}: no certificates found", path.display()));
	}
	blocks
		.iter()
		.map(|block| {
			reqwest::Certificate::from_pem(block.as_bytes()).map_err(|error| format!("{}: {error}", path.display()))
		})
		.collect()
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

pub struct Home {
	client: reqwest::Client,
	base: String,
}

impl Home {
	pub fn new(base: &str) -> Result<Self, String> {
		let url = Url::parse(base).map_err(|error| format!("invalid home url: {error}"))?;
		if url.scheme() != "https" && url.scheme() != "http" {
			return Err(format!("home url must use http or https, not `{}`", url.scheme()));
		}
		if url.scheme() == "http" && !is_loopback(url.host_str()) {
			return Err(format!(
				"{base} would send your API key and artifact bytes in the clear; use https, or a loopback address for a local instance"
			));
		}
		let mut builder = reqwest::Client::builder()
			.timeout(Duration::from_secs(30))
			.redirect(reqwest::redirect::Policy::none());
		if let Ok(path) = std::env::var("MORAINE_TLS_EXTRA_ROOTS") {
			for certificate in extra_roots(Path::new(&path))? {
				builder = builder.add_root_certificate(certificate);
			}
		}
		let client = builder.build().map_err(|error| error.to_string())?;
		Ok(Self {
			client,
			base: base.trim_end_matches('/').to_string(),
		})
	}

	pub fn base(&self) -> &str {
		&self.base
	}

	pub async fn post_wire(&self, path: &str, body: Vec<u8>) -> Result<serde_json::Value, String> {
		self.post_wire_with_token(path, body, None).await
	}

	pub async fn post_wire_status(
		&self,
		path: &str,
		body: Vec<u8>,
	) -> Result<(reqwest::StatusCode, serde_json::Value), String> {
		let response = self
			.client
			.post(format!("{}{path}", self.base))
			.header(reqwest::header::CONTENT_TYPE, "application/vnd.moraine.object+cbor")
			.body(body)
			.send()
			.await
			.map_err(|error| error.to_string())?;
		let status = response.status();
		let text = response.text().await.map_err(|error| error.to_string())?;
		let value = if status.is_success() {
			serde_json::from_str(&text).map_err(|error| format!("home returned non-JSON: {error}"))?
		} else {
			serde_json::from_str(&text).unwrap_or(serde_json::Value::Null)
		};
		Ok((status, value))
	}

	pub async fn post_wire_with_token(
		&self,
		path: &str,
		body: Vec<u8>,
		token: Option<&str>,
	) -> Result<serde_json::Value, String> {
		let mut request = self
			.client
			.post(format!("{}{path}", self.base))
			.header(reqwest::header::CONTENT_TYPE, "application/vnd.moraine.object+cbor")
			.body(body);
		if let Some(token) = token {
			request = request.bearer_auth(token);
		}
		let response = request.send().await.map_err(|error| error.to_string())?;
		read_json(response).await
	}

	pub async fn post_json_with_token(
		&self,
		path: &str,
		body: String,
		token: Option<&str>,
	) -> Result<serde_json::Value, String> {
		let mut request = self
			.client
			.post(format!("{}{path}", self.base))
			.header(reqwest::header::CONTENT_TYPE, "application/json")
			.body(body);
		if let Some(token) = token {
			request = request.bearer_auth(token);
		}
		let response = request.send().await.map_err(|error| error.to_string())?;
		read_json(response).await
	}

	pub async fn post_blob(
		&self,
		path: &str,
		file: tokio::fs::File,
		length: u64,
		token: Option<&str>,
	) -> Result<serde_json::Value, String> {
		let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
		let mut request = self
			.client
			.post(format!("{}{path}", self.base))
			.header(reqwest::header::CONTENT_LENGTH, length)
			.header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
			.body(body);
		if let Some(token) = token {
			request = request.bearer_auth(token);
		}
		let response = request.send().await.map_err(|error| error.to_string())?;
		read_json(response).await
	}

	pub async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
		let response = self
			.client
			.get(format!("{}{path}", self.base))
			.send()
			.await
			.map_err(|error| error.to_string())?;
		read_json(response).await
	}
}

async fn read_json(response: reqwest::Response) -> Result<serde_json::Value, String> {
	let status = response.status();
	let text = response.text().await.map_err(|error| error.to_string())?;
	if !status.is_success() {
		return Err(format!("home returned {status}: {text}"));
	}
	serde_json::from_str(&text).map_err(|error| format!("home returned non-JSON: {error}"))
}

fn is_loopback(host: Option<&str>) -> bool {
	let Some(host) = host else { return false };
	if host.eq_ignore_ascii_case("localhost") {
		return true;
	}
	host.strip_prefix('[')
		.and_then(|rest| rest.strip_suffix(']'))
		.unwrap_or(host)
		.parse::<std::net::IpAddr>()
		.is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn splits_a_bundle_into_blocks() {
		let text = "-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\nnoise\n-----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----\n";
		assert_eq!(pem_blocks(text).len(), 2);
	}

	#[test]
	fn rejects_a_file_with_no_certificates() {
		let directory = tempfile::tempdir().expect("tempdir");
		let path = directory.path().join("roots.pem");
		std::fs::write(&path, "not a certificate").expect("write");
		assert!(extra_roots(&path).is_err());
	}

	#[test]
	fn refuses_plaintext_for_a_remote_home() {
		assert!(Home::new("http://example.com").is_err());
		assert!(Home::new("http://127.0.0.1:8080").is_ok());
		assert!(Home::new("https://example.com").is_ok());
	}
}
