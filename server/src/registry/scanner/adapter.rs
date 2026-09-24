use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use serde::Serialize;
use tokio::process::Command;

use super::Provider;

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
	pub provider: String,
	pub kind: String,
	pub verdict: String,
	pub findings: Vec<serde_json::Value>,
	pub exit_code: Option<i32>,
	pub raw: serde_json::Value,
}

pub trait ScannerAdapter: Send + Sync {
	fn scan<'a>(&'a self, artifact: &'a Path) -> Pin<Box<dyn Future<Output = Result<ScanResult, String>> + Send + 'a>>;
}

struct CommandAdapter {
	provider: Provider,
}

impl ScannerAdapter for CommandAdapter {
	fn scan<'a>(&'a self, artifact: &'a Path) -> Pin<Box<dyn Future<Output = Result<ScanResult, String>> + Send + 'a>> {
		Box::pin(async move {
			let output = Command::new(&self.provider.command)
				.args(&self.provider.args)
				.arg(artifact)
				.output()
				.await
				.map_err(|error| error.to_string())?;
			Ok(normalize_result(
				&self.provider,
				output.status.code().unwrap_or(-1),
				&output.stdout,
				&output.stderr,
			))
		})
	}
}

pub(super) fn adapter_for(provider: Provider) -> Box<dyn ScannerAdapter> {
	Box::new(CommandAdapter { provider })
}

fn normalize_result(provider: &Provider, code: i32, stdout: &[u8], stderr: &[u8]) -> ScanResult {
	let stdout = String::from_utf8_lossy(stdout).chars().take(16_384).collect::<String>();
	let stderr = String::from_utf8_lossy(stderr).chars().take(16_384).collect::<String>();
	let lower = stdout.to_ascii_lowercase();
	let verdict = match provider.kind.as_str() {
		"clamav" => match code {
			0 => "clean",
			1 => "finding",
			_ => "error",
		},
		"neko" => {
			if code != 0
				|| ["infected", "fractureiser", "malware"]
					.iter()
					.any(|marker| lower.contains(marker))
			{
				"finding"
			} else {
				"clean"
			}
		}
		_ => match code {
			0 => "clean",
			1 => "finding",
			_ => "error",
		},
	};
	let mut findings = Vec::new();
	for line in stdout.lines() {
		let line = line.trim();
		if line.is_empty() || (!line.to_ascii_lowercase().contains("found") && provider.kind == "clamav") {
			continue;
		}
		if verdict == "finding" {
			findings.push(serde_json::json!({"message": line}));
		}
	}
	if verdict == "finding" && findings.is_empty() {
		findings.push(serde_json::json!({"message": "scanner reported a finding"}));
	}
	ScanResult {
		provider: provider.id.clone(),
		kind: provider.kind.clone(),
		verdict: verdict.to_string(),
		findings,
		exit_code: Some(code),
		raw: serde_json::json!({"stdout": stdout, "stderr": stderr}),
	}
}

#[cfg(test)]
mod tests {
	use super::{ScanResult, normalize_result};
	use crate::registry::scanner::Provider;

	fn provider(kind: &str) -> Provider {
		Provider {
			id: "test".to_string(),
			kind: kind.to_string(),
			command: "scanner".to_string(),
			args: Vec::new(),
			public_key: Vec::new(),
			enabled: true,
			local: true,
		}
	}

	#[test]
	fn normalizes_clamav_output_without_losing_raw_evidence() {
		let result: ScanResult = normalize_result(&provider("clamav"), 1, b"artifact.jar: Test.Signature FOUND\n", b"");
		assert_eq!(result.verdict, "finding");
		assert_eq!(result.findings[0]["message"], "artifact.jar: Test.Signature FOUND");
		assert_eq!(result.raw["stdout"], "artifact.jar: Test.Signature FOUND\n");
	}

	#[test]
	fn normalizes_neko_findings_even_when_the_tool_exits_successfully() {
		let result = normalize_result(&provider("neko"), 0, b"infected: fractureiser stage 0\n", b"");
		assert_eq!(result.verdict, "finding");
	}
}
