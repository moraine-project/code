mod blobs;
mod health;
mod middleware;
mod state;
mod static_files;

use axum::Router;
pub(crate) use blobs::parse_range;
pub use state::AppState;

pub fn router(state: AppState) -> Router {
	let web_dir = state.web_dir.clone();
	let web_origins = state.capability.web_origins.clone();
	let app = Router::new()
		.merge(health::routes())
		.merge(blobs::routes())
		.merge(crate::registry::routes())
		.merge(crate::auth::routes())
		.merge(crate::registry::review::routes())
		.merge(crate::federation::routes())
		.merge(crate::registry::search::routes())
		.merge(crate::auth::orgs::routes())
		.merge(crate::registry::advisories::routes())
		.merge(crate::registry::attestations::routes())
		.merge(crate::federation::mirrors::routes())
		.merge(crate::federation::notifications::routes())
		.merge(crate::federation::webhooks::routes())
		.merge(crate::registry::definitions::routes())
		.merge(crate::ops::metrics::routes())
		.merge(crate::ops::overview::routes());
	let app = static_files::fallback(app, web_dir);
	middleware::apply(app, state, &web_origins)
}
