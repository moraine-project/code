mod delivery;
mod events;
mod routes;
mod store;
mod validation;

pub use delivery::deliver_pending;
pub(crate) use events::enqueue_event;

pub fn routes() -> axum::Router<crate::routes::AppState> {
	routes::routes()
}

fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

pub(crate) fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
