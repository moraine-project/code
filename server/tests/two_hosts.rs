use std::process::{Child, Command, Stdio};
use std::time::Duration;

use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::ReleasePayload;
use moraine_model::signed::sign_payload;

struct Host {
	child: Child,
	base: String,
}

impl Drop for Host {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}

fn free_port() -> u16 {
	let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
	listener.local_addr().expect("addr").port()
}

fn spawn(data_dir: &std::path::Path, port: u16, extra: &[(&str, &str)]) -> Host {
	let mut command = Command::new(env!("CARGO_BIN_EXE_moraine-server"));
	command
		.arg("--data-dir")
		.arg(data_dir)
		.arg("--bind")
		.arg(format!("127.0.0.1:{port}"))
		.env_remove("MORAINE_DATABASE_URL")
		.env_remove("MORAINE_WEB_DIR")
		.env_remove("MORAINE_TLS_TERMINATED")
		.env_remove("MORAINE_TLS_EXTRA_ROOTS")
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	for (key, value) in extra {
		command.env(key, value);
	}
	let child = command.spawn().expect("spawn server");
	Host {
		child,
		base: format!("http://127.0.0.1:{port}"),
	}
}

async fn ready(client: &reqwest::Client, host: &Host) {
	let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
	loop {
		if let Ok(response) = client.get(format!("{}/healthz", host.base)).send().await
			&& response.status().is_success()
		{
			return;
		}
		assert!(tokio::time::Instant::now() < deadline, "{} never became ready", host.base);
		tokio::time::sleep(Duration::from_millis(100)).await;
	}
}

fn game_id(label: &str) -> String {
	format!("gd:sha256:{}", hex::encode(object_id(Kind::Release, label.as_bytes())))
}

fn genesis_wire(signer: &SigningKey) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: vec![0x33; 16],
		roots: vec![RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, &[signer]).wire_bytes()
}

fn release_wire(signer: &SigningKey, project_id: &str) -> (Vec<u8>, [u8; 32]) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: game_id("minecraft"),
		release_nonce: vec![0x44; 16],
		human_version: "1.0.0".to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: 1_760_000_000,
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
			loader_id: None,
			loader_version_predicate: None,
			side: Side::Both,
			runtime_predicate: None,
			os_predicate: None,
			arch_predicate: None,
		}],
		artifacts: vec![Artifact {
			digest: vec![0xAB; 32],
			size: 10,
			media_type: "application/java-archive".to_string(),
			filename: "example.jar".to_string(),
			is_primary: true,
			os_predicate: None,
			arch_predicate: None,
		}],
		dependencies: Vec::new(),
		source_reference: None,
		changelog_digest: None,
		license_expression: None,
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: Vec::new(),
	};
	let signed = sign_payload(Kind::Release, &release, &[signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	(signed.wire_bytes(), digest)
}

fn feed_wire(signer: &SigningKey, project_id: &str, object_digest: [u8; 32]) -> Vec<u8> {
	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence: 1,
		previous: None,
		kind: "release-published".to_string(),
		object_digest: object_digest.to_vec(),
		declared_at: 1_760_000_001,
	};
	sign_payload(Kind::FeedEntry, &entry, &[signer]).wire_bytes()
}

fn cookie(response: &reqwest::Response, name: &str) -> String {
	response
		.headers()
		.get_all("set-cookie")
		.iter()
		.filter_map(|value| value.to_str().ok())
		.filter_map(|value| value.split(';').next())
		.find_map(|pair| pair.strip_prefix(&format!("{name}=")))
		.unwrap_or_default()
		.to_string()
}

fn json(value: &serde_json::Value) -> Vec<u8> {
	serde_json::to_vec(value).expect("json")
}

async fn json_body(response: reqwest::Response) -> serde_json::Value {
	let bytes = response.bytes().await.expect("body");
	serde_json::from_slice(&bytes).expect("json")
}

fn bootstrap(data_dir: &std::path::Path) -> String {
	let output = Command::new(env!("CARGO_BIN_EXE_moraine-server"))
		.arg("--data-dir")
		.arg(data_dir)
		.arg("bootstrap")
		.arg("--email")
		.arg("ops@example.org")
		.output()
		.expect("bootstrap");
	assert!(output.status.success(), "bootstrap failed");
	let stdout = String::from_utf8_lossy(&output.stdout);
	stdout
		.lines()
		.find_map(|line| line.strip_prefix("operator password:"))
		.map(|password| password.trim().to_string())
		.expect("operator password")
}

async fn login(client: &reqwest::Client, base: &str, password: &str) -> (String, String) {
	let credentials = json(&serde_json::json!({ "email": "ops@example.org", "password": password }));
	client
		.post(format!("{base}/v1/auth/register"))
		.header("content-type", "application/json")
		.body(credentials.clone())
		.send()
		.await
		.expect("register");
	let response = client
		.post(format!("{base}/v1/auth/session"))
		.header("content-type", "application/json")
		.body(credentials)
		.send()
		.await
		.expect("session");
	assert!(response.status().is_success(), "session: {}", response.status());
	let session = cookie(&response, "moraine_session");
	let csrf = cookie(&response, "moraine_csrf");
	assert!(!session.is_empty() && !csrf.is_empty(), "no session cookies");
	(session, csrf)
}

#[tokio::test]
async fn one_process_syncs_a_feed_from_another_over_http() {
	let client = reqwest::Client::new();
	let signer = SigningKey::from_seed(&[9u8; 32]);

	let home_dir = tempfile::tempdir().expect("home dir");
	let directory_dir = tempfile::tempdir().expect("directory dir");
	let directory_password = bootstrap(directory_dir.path());
	let home = spawn(home_dir.path(), free_port(), &[("MORAINE_PUBLISHING", "open")]);
	let directory = spawn(
		directory_dir.path(),
		free_port(),
		&[
			("MORAINE_PUBLISHING", "open"),
			("MORAINE_FEDERATION_ALLOW_HTTP_LOCAL", "true"),
		],
	);
	ready(&client, &home).await;
	ready(&client, &directory).await;

	let response = client
		.post(format!("{}/v1/projects", home.base))
		.body(genesis_wire(&signer))
		.send()
		.await
		.expect("genesis");
	assert_eq!(response.status(), reqwest::StatusCode::CREATED);
	let project_id = json_body(response).await["project_id"]
		.as_str()
		.expect("project id")
		.to_string();

	let (release, digest) = release_wire(&signer, &project_id);
	let response = client
		.post(format!("{}/v1/projects/{project_id}/objects/release", home.base))
		.body(release)
		.send()
		.await
		.expect("release");
	assert_eq!(response.status(), reqwest::StatusCode::CREATED);

	let response = client
		.post(format!("{}/v1/projects/{project_id}/feed", home.base))
		.body(feed_wire(&signer, &project_id, digest))
		.send()
		.await
		.expect("feed");
	assert_eq!(response.status(), reqwest::StatusCode::CREATED);

	let (session, csrf) = login(&client, &directory.base, &directory_password).await;
	let response = client
		.post(format!("{}/v1/federation/sync", directory.base))
		.header("cookie", format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.header("content-type", "application/json")
		.body(json(&serde_json::json!({ "home_url": home.base, "project_id": project_id })))
		.send()
		.await
		.expect("sync");
	assert_eq!(response.status(), reqwest::StatusCode::OK, "sync failed");
	let report = json_body(response).await;
	assert_eq!(report["applied"], 1, "{report}");

	let page = client
		.get(format!("{}/v1/projects/{project_id}/feed", directory.base))
		.send()
		.await
		.expect("feed read");
	assert_eq!(page.status(), reqwest::StatusCode::OK, "feed read failed");
	let page = json_body(page).await;
	assert_eq!(page["head_seq"], 1);
}
