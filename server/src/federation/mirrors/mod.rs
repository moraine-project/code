mod probe;
mod routes;
mod store;

use std::time::SystemTime;

pub use probe::probe_mirrors;
pub use routes::routes;
#[cfg(test)]
pub(crate) use store::CommitmentRow;

fn id_for(digest: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(digest))
}

pub(crate) fn now() -> i64 {
	SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
