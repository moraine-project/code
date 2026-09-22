use crate::db::MetadataStore;
use crate::registry::search_index::*;

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn counts_same_names_across_the_whole_index() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		for (id, name, game) in [
			("a", "Example Mod", "game"),
			("b", "example-mod", "game"),
			("c", "Example Mod", "other"),
		] {
			store
				.put_search_document(SearchDocument {
					project_id: id,
					game_id: game,
					display_name: name,
					summary: "",
					description: "",
					categories: &[],
					tags: &[],
					updated_at: 0,
				})
				.await
				.expect("document");
		}

		let counts = store
			.name_collision_counts(&[
				("game".to_string(), "examplemod".to_string()),
				("other".to_string(), "examplemod".to_string()),
				("game".to_string(), "unrelated".to_string()),
			])
			.await
			.expect("counts");

		assert_eq!(counts.get(&("game".to_string(), "examplemod".to_string())), Some(&2));
		assert_eq!(counts.get(&("other".to_string(), "examplemod".to_string())), Some(&1));
		assert_eq!(counts.get(&("game".to_string(), "unrelated".to_string())), None);
	}

	#[tokio::test]
	async fn matches_text_that_only_appears_in_a_changelog() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		for (id, name) in [("a", "Widget"), ("b", "Gadget")] {
			store
				.put_search_document(SearchDocument {
					project_id: id,
					game_id: "game",
					display_name: name,
					summary: "Summary",
					description: "Description",
					categories: &[],
					tags: &[],
					updated_at: 0,
				})
				.await
				.expect("document");
		}
		store
			.put_changelog_text(&[0x77; 32], "b", "Fixes\nRemoved the kraken crash")
			.await
			.expect("changelog");

		let hits = store
			.search_documents(SearchFilter {
				text: Some("kraken"),
				game_id: None,
				tag: None,
				category: None,
				loader: None,
				game_version: None,
				loader_version: None,
				runtime_version: None,
				channel: None,
				platform: None,
				popularity_since: None,
				sort: SearchSort::Relevance,
				cursor: None,
				limit: 10,
			})
			.await
			.expect("search");

		assert_eq!(hits.len(), 1);
		assert_eq!(hits[0].project_id, "b");
	}

	#[tokio::test]
	async fn a_name_prefix_outranks_a_description_substring() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		for (id, name, description) in [("a", "Fabric Addon", "nothing relevant"), ("b", "Other", "a fabric helper")] {
			store
				.put_search_document(SearchDocument {
					project_id: id,
					game_id: "game",
					display_name: name,
					summary: "Summary",
					description,
					categories: &[],
					tags: &[],
					updated_at: 0,
				})
				.await
				.expect("document");
		}

		let hits = store
			.search_documents(SearchFilter {
				text: Some("fab"),
				game_id: None,
				tag: None,
				category: None,
				loader: None,
				game_version: None,
				loader_version: None,
				runtime_version: None,
				channel: None,
				platform: None,
				popularity_since: None,
				sort: SearchSort::Relevance,
				cursor: None,
				limit: 10,
			})
			.await
			.expect("search");

		assert_eq!(hits.len(), 2);
		assert_eq!(hits[0].project_id, "a");
	}

	#[tokio::test]
	async fn a_like_wildcard_is_matched_literally() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.put_search_document(SearchDocument {
				project_id: "a",
				game_id: "game",
				display_name: "Ordinary",
				summary: "Summary",
				description: "Description",
				categories: &[],
				tags: &[],
				updated_at: 0,
			})
			.await
			.expect("document");

		let hits = store
			.search_documents(SearchFilter {
				text: Some("%"),
				game_id: None,
				tag: None,
				category: None,
				loader: None,
				game_version: None,
				loader_version: None,
				runtime_version: None,
				channel: None,
				platform: None,
				popularity_since: None,
				sort: SearchSort::Relevance,
				cursor: None,
				limit: 10,
			})
			.await
			.expect("search");

		assert!(hits.is_empty(), "a bare percent matches nothing");
	}
}

#[cfg(test)]
mod home_tests {
	use super::*;

	#[tokio::test]
	async fn reports_the_home_a_project_was_followed_from() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.upsert_subscription("https://older.example", "p", "active", 1)
			.await
			.expect("subscribe");
		store
			.upsert_subscription("https://newer.example", "p", "active", 5)
			.await
			.expect("subscribe");

		let homes = store.project_homes(&["p".to_string(), "q".to_string()]).await.expect("homes");

		assert_eq!(homes.get("p"), Some(&"https://newer.example".to_string()));
		assert_eq!(homes.get("q"), None);
	}
}
