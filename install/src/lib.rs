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

/// Computes where each file goes for a game adapter. It only plans; writing to
/// a user's disk is the launcher's job, after the bytes are verified.
pub fn plan(adapter: &str, mods: &[ModFile], overrides: &[OverrideFile]) -> Result<InstallPlan, InstallError> {
	match adapter {
		MINECRAFT_ADAPTER => plan_minecraft(mods, overrides),
		other => Err(InstallError::UnknownAdapter(other.to_string())),
	}
}

fn plan_minecraft(mods: &[ModFile], overrides: &[OverrideFile]) -> Result<InstallPlan, InstallError> {
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
	Ok(InstallPlan {
		adapter: MINECRAFT_ADAPTER.to_string(),
		placements,
	})
}

/// Joins a planned relative path onto an instance root and guarantees the
/// result stays inside that root. A path is rejected, not normalized, if it
/// escapes.
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

/// A mod file name is a single path segment with no traversal or separators.
pub fn safe_filename(name: &str) -> Option<&str> {
	if name.is_empty()
		|| name == "."
		|| name == ".."
		|| name.contains('/')
		|| name.contains('\\')
		|| name.contains(':')
		|| name.contains('\0')
	{
		return None;
	}
	Some(name)
}

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
