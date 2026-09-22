mod feed;
mod objects;
mod projects;

use axum::Router;
use axum::routing::{get, post};
pub(crate) use feed::{
	bad_request, id_for, ingest_feed, load_delegations, load_root, parse_hex_digest, prepare_feed, storage_error,
	store_object_record, stored, unix_now,
};

use crate::registry::{
	bundle, channels, deny_lists, external, grants, impersonation, legal, loader_accepts, migration, policy, profile,
	recovery, sanctions, scanner,
};
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/projects", post(projects::create_project))
		.route("/v1/projects/{id}", get(projects::project_summary))
		.route("/v1/projects/{id}/objects/{kind}", post(objects::store_object))
		.route(
			"/v1/projects/{id}/feed",
			get(crate::registry::feed::page).post(feed::append_feed),
		)
		.route("/v1/projects/{id}/transfer", post(projects::transfer))
		.merge(crate::registry::views::routes())
		.merge(profile::routes())
		.merge(bundle::routes())
		.merge(policy::routes())
		.merge(legal::routes())
		.merge(loader_accepts::routes())
		.merge(channels::routes())
		.merge(deny_lists::routes())
		.merge(migration::routes())
		.merge(recovery::routes())
		.merge(impersonation::routes())
		.merge(sanctions::routes())
		.merge(scanner::routes())
		.merge(grants::routes())
		.merge(external::routes())
}
