pub mod advisories;
pub mod artifacts;
pub mod attestations;
pub mod bundle;
mod catalog;
pub mod channels;
pub mod compatibility;
pub mod definitions;
pub mod deny_lists;
pub mod external;
pub mod feed;
pub mod grants;
pub mod impersonation;
pub mod legal;
pub mod loader_accepts;
pub mod loader_releases;
pub mod migration;
pub mod policy;
pub mod profile;
pub mod recovery;
pub mod review;
pub mod sanctions;
pub mod scanner;
pub mod search;
pub mod search_facets;
pub mod search_index;
pub mod search_labels;
pub mod views;
pub mod witness;

mod project;

pub(crate) const OBJECT_CONTENT_TYPE: &str = "application/vnd.moraine.object+cbor";

use axum::Router;
pub(crate) use project::{
	id_for, ingest_feed, load_delegations, load_root, parse_hex_digest, prepare_feed, storage_error, store_object_record,
	stored,
};

use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	project::routes()
}
