pub mod advisories;
pub mod artifacts;
pub mod attestations;
pub mod bundle;
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

mod project_feed;

pub(crate) const OBJECT_CONTENT_TYPE: &str = "application/vnd.moraine.object+cbor";

use axum::Router;
pub(crate) use project_feed::{
	id_for, ingest_feed, load_delegations, load_root, parse_hex_digest, prepare_feed, storage_error, store_object_record,
	stored,
};

use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	project_feed::routes()
}

#[cfg(test)]
mod definition_tests;

#[cfg(test)]
mod deny_lists_tests;

#[cfg(test)]
mod external_tests;

#[cfg(test)]
mod feed_tests;

#[cfg(test)]
mod admission_tests;

#[cfg(test)]
mod attestations_tests;

#[cfg(test)]
mod bundle_tests;

#[cfg(test)]
mod channels_tests;

#[cfg(test)]
mod impersonation_tests;

#[cfg(test)]
mod legal_tests;

#[cfg(test)]
mod loader_accepts_tests;

#[cfg(test)]
mod ownership_tests;

#[cfg(test)]
mod migration_tests;

#[cfg(test)]
mod policy_tests;

#[cfg(test)]
mod profile_tests;

#[cfg(test)]
mod sanctions_tests;

#[cfg(test)]
mod recovery_tests;

#[cfg(test)]
mod search_tests;

#[cfg(test)]
mod views_tests;
