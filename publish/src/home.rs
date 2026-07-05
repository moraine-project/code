use std::time::Duration;

use reqwest::Url;

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
		let client = reqwest::Client::builder()
			.timeout(Duration::from_secs(30))
			.build()
			.map_err(|error| error.to_string())?;
		Ok(Self {
			client,
			base: base.trim_end_matches('/').to_string(),
		})
	}

	pub fn base(&self) -> &str {
		&self.base
	}

	pub async fn post_wire(&self, path: &str, body: Vec<u8>) -> Result<serde_json::Value, String> {
		let response = self
			.client
			.post(format!("{}{path}", self.base))
			.header(reqwest::header::CONTENT_TYPE, "application/vnd.moraine.object+cbor")
			.body(body)
			.send()
			.await
			.map_err(|error| error.to_string())?;
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
