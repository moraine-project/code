use std::io::{Cursor, Read};

use zip::ZipArchive;

const MAX_ENTRY_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModMetadata {
	pub loader: Option<String>,
	pub mod_id: Option<String>,
	pub name: Option<String>,
	pub version: Option<String>,
	pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
	NotAnArchive,
	Archive(String),
	EntryTooLarge(String),
	UnknownExtractor(String),
}

impl std::fmt::Display for MetadataError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::NotAnArchive => f.write_str("file is not a zip archive"),
			Self::Archive(detail) => write!(f, "archive error: {detail}"),
			Self::EntryTooLarge(name) => write!(f, "`{name}` is larger than the metadata limit"),
			Self::UnknownExtractor(name) => write!(f, "unknown metadata extractor `{name}`"),
		}
	}
}

impl std::error::Error for MetadataError {}

pub const FABRIC_EXTRACTOR: &str = "minecraft/fabric-json";
pub const QUILT_EXTRACTOR: &str = "minecraft/quilt-json";
pub const FORGE_EXTRACTOR: &str = "minecraft/forge-toml";

pub fn extract(bytes: &[u8]) -> Result<ModMetadata, MetadataError> {
	let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| MetadataError::NotAnArchive)?;
	if let Some(metadata) = fabric(&mut archive)? {
		return Ok(metadata);
	}
	if let Some(metadata) = quilt(&mut archive)? {
		return Ok(metadata);
	}
	if let Some(metadata) = forge(&mut archive)? {
		return Ok(metadata);
	}
	Ok(ModMetadata::default())
}

pub fn extract_with(extractor: &str, bytes: &[u8]) -> Result<ModMetadata, MetadataError> {
	let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| MetadataError::NotAnArchive)?;
	match extractor {
		FABRIC_EXTRACTOR => Ok(fabric(&mut archive)?.unwrap_or_default()),
		QUILT_EXTRACTOR => Ok(quilt(&mut archive)?.unwrap_or_default()),
		FORGE_EXTRACTOR => Ok(forge(&mut archive)?.unwrap_or_default()),
		other => Err(MetadataError::UnknownExtractor(other.to_string())),
	}
}

pub const KNOWN_EXTRACTORS: &[&str] = &[FABRIC_EXTRACTOR, QUILT_EXTRACTOR, FORGE_EXTRACTOR];

pub fn is_known_extractor(extractor: &str) -> bool {
	KNOWN_EXTRACTORS.contains(&extractor)
}

fn read_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>, MetadataError> {
	let Ok(mut file) = archive.by_name(name) else {
		return Ok(None);
	};
	if file.size() > MAX_ENTRY_BYTES {
		return Err(MetadataError::EntryTooLarge(name.to_string()));
	}
	let mut buffer = Vec::with_capacity(file.size() as usize);
	file.read_to_end(&mut buffer)
		.map_err(|error| MetadataError::Archive(error.to_string()))?;
	Ok(Some(buffer))
}

fn fabric(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Option<ModMetadata>, MetadataError> {
	let Some(bytes) = read_entry(archive, "fabric.mod.json")? else {
		return Ok(None);
	};
	let document: serde_json::Value =
		serde_json::from_slice(&bytes).map_err(|error| MetadataError::Archive(error.to_string()))?;
	Ok(Some(ModMetadata {
		loader: Some("fabric".to_string()),
		mod_id: string_field(&document, "id"),
		name: string_field(&document, "name"),
		version: string_field(&document, "version"),
		environment: string_field(&document, "environment"),
	}))
}

fn quilt(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Option<ModMetadata>, MetadataError> {
	let Some(bytes) = read_entry(archive, "quilt.mod.json")? else {
		return Ok(None);
	};
	let document: serde_json::Value =
		serde_json::from_slice(&bytes).map_err(|error| MetadataError::Archive(error.to_string()))?;
	let loader = document.get("quilt_loader");
	Ok(Some(ModMetadata {
		loader: Some("quilt".to_string()),
		mod_id: loader.and_then(|value| string_field(value, "id")),
		name: document.get("metadata").and_then(|value| string_field(value, "name")),
		version: loader.and_then(|value| string_field(value, "version")),
		environment: None,
	}))
}

fn forge(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Option<ModMetadata>, MetadataError> {
	let Some(bytes) = read_entry(archive, "META-INF/mods.toml")? else {
		return Ok(None);
	};
	let text = String::from_utf8(bytes).map_err(|_| MetadataError::Archive("mods.toml is not UTF-8".to_string()))?;
	let document: toml::Value = toml::from_str(&text).map_err(|error| MetadataError::Archive(error.to_string()))?;
	let entry = document
		.get("mods")
		.and_then(|mods| mods.as_array())
		.and_then(|mods| mods.first());
	Ok(Some(ModMetadata {
		loader: Some("forge".to_string()),
		mod_id: entry
			.and_then(|entry| entry.get("modId"))
			.and_then(|value| value.as_str())
			.map(str::to_string),
		name: entry
			.and_then(|entry| entry.get("displayName"))
			.and_then(|value| value.as_str())
			.map(str::to_string),
		version: entry
			.and_then(|entry| entry.get("version"))
			.and_then(|value| value.as_str())
			.map(str::to_string),
		environment: None,
	}))
}

fn string_field(document: &serde_json::Value, key: &str) -> Option<String> {
	document.get(key).and_then(|value| value.as_str()).map(str::to_string)
}

#[cfg(test)]
mod tests {
	use std::io::Write;

	use zip::write::SimpleFileOptions;

	use super::*;

	fn archive_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
		let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
		let options = SimpleFileOptions::default();
		for (name, body) in entries {
			writer.start_file(*name, options).expect("start");
			writer.write_all(body).expect("write");
		}
		writer.finish().expect("finish").into_inner()
	}

	#[test]
	fn reads_a_fabric_manifest() {
		let manifest = br#"{"id":"example","name":"Example Mod","version":"1.2.3","environment":"client"}"#;
		let bytes = archive_with(&[("fabric.mod.json", manifest)]);
		let metadata = extract(&bytes).expect("extract");
		assert_eq!(metadata.loader.as_deref(), Some("fabric"));
		assert_eq!(metadata.mod_id.as_deref(), Some("example"));
		assert_eq!(metadata.version.as_deref(), Some("1.2.3"));
		assert_eq!(metadata.environment.as_deref(), Some("client"));
	}

	#[test]
	fn reads_a_forge_manifest() {
		let manifest = b"[[mods]]\nmodId=\"example\"\ndisplayName=\"Example\"\nversion=\"4.5.6\"\n";
		let bytes = archive_with(&[("META-INF/mods.toml", manifest)]);
		let metadata = extract(&bytes).expect("extract");
		assert_eq!(metadata.loader.as_deref(), Some("forge"));
		assert_eq!(metadata.mod_id.as_deref(), Some("example"));
		assert_eq!(metadata.version.as_deref(), Some("4.5.6"));
	}

	#[test]
	fn a_named_extractor_reads_only_its_own_format() {
		let manifest = br#"{"id":"example","name":"Example Mod","version":"1.2.3"}"#;
		let bytes = archive_with(&[("fabric.mod.json", manifest)]);
		let metadata = extract_with(FABRIC_EXTRACTOR, &bytes).expect("extract");
		assert_eq!(metadata.mod_id.as_deref(), Some("example"));

		let forge_toml = b"[[mods]]\nmodId=\"other\"\n";
		let bytes = archive_with(&[("META-INF/mods.toml", forge_toml)]);
		let metadata = extract_with(FABRIC_EXTRACTOR, &bytes).expect("extract");
		assert_eq!(metadata.mod_id, None);

		assert!(matches!(
			extract_with("minecraft/nonsense", &bytes),
			Err(MetadataError::UnknownExtractor(_))
		));
	}

	#[test]
	fn rejects_a_non_archive() {
		assert_eq!(extract(b"not a zip"), Err(MetadataError::NotAnArchive));
	}

	#[test]
	fn refuses_an_oversized_metadata_entry() {
		let big = vec![b'a'; (MAX_ENTRY_BYTES + 1) as usize];
		let bytes = archive_with(&[("fabric.mod.json", &big)]);
		assert!(matches!(extract(&bytes), Err(MetadataError::EntryTooLarge(_))));
	}
}
