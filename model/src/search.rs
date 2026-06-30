use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_LIMIT: u32 = 100;
pub const POPULARITY_WINDOW: &str = "30d";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListingState {
	Listed,
	Unlisted,
	Quarantined,
	Blocked,
	Withdrawn,
	Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstancePopularity {
	pub window: String,
	pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
	pub kind: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ref_digest: Option<String>,
	pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub text: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub game: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub loader: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub category: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub tag: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub game_version: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub sort: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cursor: Option<String>,
	pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
	pub project_id: String,
	pub game_id: String,
	pub display_name: String,
	pub summary: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub icon_url: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub latest_release_id: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub matched_release_id: Option<String>,
	pub listing_state: ListingState,
	pub source_instance: String,
	#[serde(default)]
	pub annotations: Vec<Annotation>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub instance_popularity: Option<InstancePopularity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResponse {
	pub protocol: u32,
	pub query: SearchQuery,
	pub results: Vec<SearchResult>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub next_cursor: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub total_estimate: Option<u64>,
}

impl SearchResponse {
	pub fn validate(&self) -> Result<(), &'static str> {
		if self.protocol != PROTOCOL_VERSION {
			return Err("unsupported search protocol version");
		}
		if self.query.limit == 0 || self.query.limit > MAX_LIMIT {
			return Err("search limit out of range");
		}
		if let Some(popularity) = self.results.iter().find_map(|result| result.instance_popularity.as_ref())
			&& popularity.window != POPULARITY_WINDOW
		{
			return Err("popularity window must be instance-local and windowed");
		}
		Ok(())
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceListing {
	pub source_instance: String,
	pub listing_state: ListingState,
	pub latest_release_id: Option<String>,
	pub matched_release_id: Option<String>,
	pub annotations: Vec<Annotation>,
	pub instance_popularity: Option<InstancePopularity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedResult {
	pub project_id: String,
	pub game_id: String,
	pub display_name: String,
	pub summary: String,
	pub icon_url: Option<String>,
	pub listings: Vec<SourceListing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergedSearch {
	pub results: Vec<MergedResult>,
}

/// Merge keeps attribution: one project ID yields one result, and every source
/// keeps its own listing state and annotations. Popularity is never summed or
/// averaged across instances, because no shared user identity exists to
/// deduplicate it.
pub fn merge(responses: impl IntoIterator<Item = SearchResponse>) -> MergedSearch {
	let mut merged = MergedSearch::default();
	for response in responses {
		for result in response.results {
			let listing = SourceListing {
				source_instance: result.source_instance,
				listing_state: result.listing_state,
				latest_release_id: result.latest_release_id,
				matched_release_id: result.matched_release_id,
				annotations: result.annotations,
				instance_popularity: result.instance_popularity,
			};
			match merged
				.results
				.iter_mut()
				.find(|existing| existing.project_id == result.project_id)
			{
				Some(existing) => existing.listings.push(listing),
				None => merged.results.push(MergedResult {
					project_id: result.project_id,
					game_id: result.game_id,
					display_name: result.display_name,
					summary: result.summary,
					icon_url: result.icon_url,
					listings: vec![listing],
				}),
			}
		}
	}
	merged
}

#[cfg(test)]
mod tests {
	use super::*;

	fn result(project_id: &str, instance: &str, state: ListingState) -> SearchResult {
		SearchResult {
			project_id: project_id.to_string(),
			game_id: "g".to_string(),
			display_name: "Example".to_string(),
			summary: "s".to_string(),
			icon_url: None,
			latest_release_id: None,
			matched_release_id: None,
			listing_state: state,
			source_instance: instance.to_string(),
			annotations: Vec::new(),
			instance_popularity: None,
		}
	}

	fn response(results: Vec<SearchResult>) -> SearchResponse {
		SearchResponse {
			protocol: 1,
			query: SearchQuery {
				text: None,
				game: None,
				loader: None,
				category: None,
				tag: None,
				game_version: None,
				sort: None,
				cursor: None,
				limit: 20,
			},
			results,
			next_cursor: None,
			total_estimate: None,
		}
	}

	#[test]
	fn merge_groups_by_project_id_and_keeps_conflicting_states() {
		let a = response(vec![
			result("p1", "dir-a", ListingState::Listed),
			result("p2", "dir-a", ListingState::Listed),
		]);
		let b = response(vec![result("p1", "dir-b", ListingState::Blocked)]);
		let merged = merge([a, b]);
		assert_eq!(merged.results.len(), 2);
		let p1 = merged
			.results
			.iter()
			.find(|project| project.project_id == "p1")
			.expect("p1 merged");
		assert_eq!(p1.listings.len(), 2);
		assert!(
			p1.listings
				.iter()
				.any(|listing| listing.listing_state == ListingState::Listed)
		);
		assert!(
			p1.listings
				.iter()
				.any(|listing| listing.listing_state == ListingState::Blocked)
		);
	}

	#[test]
	fn validate_rejects_out_of_range_limits() {
		let mut response = response(vec![result("p1", "dir-a", ListingState::Listed)]);
		response.query.limit = 0;
		assert!(response.validate().is_err());
		response.query.limit = MAX_LIMIT + 1;
		assert!(response.validate().is_err());
		response.query.limit = MAX_LIMIT;
		assert!(response.validate().is_ok());
	}
}
