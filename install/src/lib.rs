use std::path::{Component, Path, PathBuf};

use moraine_model::modpack::valid_override_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModFile {
	pub digest: [u8; 32],
	pub filename: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideFile {
	pub digest: [u8; 32],
	pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
	pub digest: [u8; 32],
	pub relative_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
	pub adapter: String,
	pub placements: Vec<Placement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
	UnknownAdapter(String),
	UnsafeFilename(String),
	UnsafePath(String),
}

impl std::fmt::Display for InstallError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownAdapter(adapter) => write!(f, "unknown install adapter `{adapter}`"),
			Self::UnsafeFilename(name) => write!(f, "unsafe artifact filename `{name}`"),
			Self::UnsafePath(path) => write!(f, "unsafe override path `{path}`"),
		}
	}
}

impl std::error::Error for InstallError {}

pub const MINECRAFT_ADAPTER: &str = "minecraft/default";
pub const SIMS4_ADAPTER: &str = "sims4/default";

pub const KNOWN_ADAPTERS: &[&str] = &[MINECRAFT_ADAPTER, SIMS4_ADAPTER];

pub fn is_known_adapter(adapter: &str) -> bool {
	KNOWN_ADAPTERS.contains(&adapter)
}

pub fn plan(adapter: &str, mods: &[ModFile], overrides: &[OverrideFile]) -> Result<InstallPlan, InstallError> {
	let placements = match adapter {
		MINECRAFT_ADAPTER => plan_minecraft(mods, overrides)?,
		SIMS4_ADAPTER => plan_sims4(mods, overrides)?,
		other => return Err(InstallError::UnknownAdapter(other.to_string())),
	};
	let mut seen = std::collections::HashSet::new();
	for placement in &placements {
		let key = placement.relative_path.to_string_lossy().to_lowercase();
		if !seen.insert(key) {
			return Err(InstallError::UnsafePath(placement.relative_path.display().to_string()));
		}
	}
	Ok(InstallPlan {
		adapter: adapter.to_string(),
		placements,
	})
}

fn plan_minecraft(mods: &[ModFile], overrides: &[OverrideFile]) -> Result<Vec<Placement>, InstallError> {
	let mut placements = Vec::with_capacity(mods.len() + overrides.len());
	for file in mods {
		let name = safe_filename(&file.filename).ok_or_else(|| InstallError::UnsafeFilename(file.filename.clone()))?;
		placements.push(Placement {
			digest: file.digest,
			relative_path: PathBuf::from("mods").join(name),
		});
	}
	for file in overrides {
		if !valid_override_path(&file.target_path) {
			return Err(InstallError::UnsafePath(file.target_path.clone()));
		}
		placements.push(Placement {
			digest: file.digest,
			relative_path: PathBuf::from(&file.target_path),
		});
	}
	Ok(placements)
}

fn plan_sims4(mods: &[ModFile], overrides: &[OverrideFile]) -> Result<Vec<Placement>, InstallError> {
	let mut placements = Vec::with_capacity(mods.len() + overrides.len());
	for file in mods {
		let name = safe_filename(&file.filename).ok_or_else(|| InstallError::UnsafeFilename(file.filename.clone()))?;
		placements.push(Placement {
			digest: file.digest,
			relative_path: PathBuf::from("Mods").join(name),
		});
	}
	for file in overrides {
		if !valid_override_path(&file.target_path) {
			return Err(InstallError::UnsafePath(file.target_path.clone()));
		}
		placements.push(Placement {
			digest: file.digest,
			relative_path: PathBuf::from("Mods").join(&file.target_path),
		});
	}
	Ok(placements)
}

pub fn safe_join(root: &Path, relative: &Path) -> Option<PathBuf> {
	let mut result = PathBuf::from(root);
	for component in relative.components() {
		match component {
			Component::Normal(segment) => result.push(segment),
			Component::CurDir => {}
			Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
		}
	}
	(result.starts_with(root)).then_some(result)
}

pub fn safe_filename(name: &str) -> Option<&str> {
	if name.is_empty()
		|| name == "."
		|| name == ".."
		|| name.contains('/')
		|| name.contains('\\')
		|| name.contains(':')
		|| name.contains('\0')
		|| name.chars().any(char::is_control)
		|| name.ends_with('.')
		|| name.ends_with(' ')
	{
		return None;
	}
	let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
	if RESERVED_NAMES.contains(&stem.as_str()) {
		return None;
	}
	Some(name)
}

const RESERVED_NAMES: &[&str] = &[
	"CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
	"LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn places_mods_and_overrides_for_minecraft() {
		let mods = vec![ModFile {
			digest: [1u8; 32],
			filename: "example.jar".to_string(),
		}];
		let overrides = vec![OverrideFile {
			digest: [2u8; 32],
			target_path: "config/example.toml".to_string(),
		}];
		let plan = plan(MINECRAFT_ADAPTER, &mods, &overrides).expect("plan");
		assert_eq!(plan.placements[0].relative_path, PathBuf::from("mods/example.jar"));
		assert_eq!(plan.placements[1].relative_path, PathBuf::from("config/example.toml"));
	}

	#[test]
	fn rejects_unsafe_filenames_and_override_paths() {
		let unsafe_mod = vec![ModFile {
			digest: [1u8; 32],
			filename: "../evil.jar".to_string(),
		}];
		assert!(matches!(
			plan(MINECRAFT_ADAPTER, &unsafe_mod, &[]),
			Err(InstallError::UnsafeFilename(_))
		));

		let unsafe_override = vec![OverrideFile {
			digest: [2u8; 32],
			target_path: "config/../../escape".to_string(),
		}];
		assert!(matches!(
			plan(MINECRAFT_ADAPTER, &[], &unsafe_override),
			Err(InstallError::UnsafePath(_))
		));
	}

	#[test]
	fn places_packages_under_the_mods_folder_for_the_sims() {
		let mods = vec![ModFile {
			digest: [3u8; 32],
			filename: "example.package".to_string(),
		}];
		let overrides = vec![OverrideFile {
			digest: [4u8; 32],
			target_path: "overrides/example.package".to_string(),
		}];
		let plan = plan(SIMS4_ADAPTER, &mods, &overrides).expect("plan");
		assert_eq!(plan.adapter, SIMS4_ADAPTER);
		assert_eq!(plan.placements[0].relative_path, PathBuf::from("Mods/example.package"));
		assert_eq!(
			plan.placements[1].relative_path,
			PathBuf::from("Mods/overrides/example.package")
		);
	}

	#[test]
	fn the_sims_adapter_still_refuses_to_escape_its_root() {
		let unsafe_mod = vec![ModFile {
			digest: [3u8; 32],
			filename: "../../Documents/evil.package".to_string(),
		}];
		assert!(matches!(
			plan(SIMS4_ADAPTER, &unsafe_mod, &[]),
			Err(InstallError::UnsafeFilename(_))
		));

		let unsafe_override = vec![OverrideFile {
			digest: [4u8; 32],
			target_path: "../../escape.package".to_string(),
		}];
		assert!(matches!(
			plan(SIMS4_ADAPTER, &[], &unsafe_override),
			Err(InstallError::UnsafePath(_))
		));
	}

	#[test]
	fn rejects_unknown_adapters() {
		assert!(matches!(plan("quake/rtx", &[], &[]), Err(InstallError::UnknownAdapter(_))));
	}

	#[test]
	fn safe_join_never_escapes_the_root() {
		let root = Path::new("/instances/one");
		assert_eq!(
			safe_join(root, Path::new("mods/example.jar")),
			Some(PathBuf::from("/instances/one/mods/example.jar"))
		);
		assert_eq!(safe_join(root, Path::new("../two/evil.jar")), None);
		assert_eq!(safe_join(root, Path::new("/etc/passwd")), None);
	}
}
