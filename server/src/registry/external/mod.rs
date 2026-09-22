mod routes;
mod store;

use std::time::SystemTime;

pub use routes::routes;

fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

pub(crate) fn now() -> i64 {
	SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
}
