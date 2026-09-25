mod events;
mod routes;
mod store;

use std::time::SystemTime;

pub(crate) use events::notify_followers;
pub use routes::routes;
#[cfg(test)]
pub(crate) use store::NotificationRow;

pub(crate) fn now() -> i64 {
	SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
