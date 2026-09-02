use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
	Genesis,
	Delegation,
	Release,
	FeedEntry,
	Profile,
	Changelog,
	Modpack,
	Advisory,
	Attestation,
	DenyList,
	GameDef,
	LoaderDef,
	RuntimeDef,
}

impl ObjectKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Genesis => "genesis",
			Self::Delegation => "delegation",
			Self::Release => "release",
			Self::FeedEntry => "feed-entry",
			Self::Profile => "profile",
			Self::Changelog => "changelog",
			Self::Modpack => "modpack",
			Self::Advisory => "advisory",
			Self::Attestation => "attestation",
			Self::DenyList => "deny-list",
			Self::GameDef => "game-def",
			Self::LoaderDef => "loader-def",
			Self::RuntimeDef => "runtime-def",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"genesis" => Self::Genesis,
			"delegation" => Self::Delegation,
			"release" => Self::Release,
			"feed-entry" => Self::FeedEntry,
			"profile" => Self::Profile,
			"changelog" => Self::Changelog,
			"modpack" => Self::Modpack,
			"advisory" => Self::Advisory,
			"attestation" => Self::Attestation,
			"deny-list" => Self::DenyList,
			"game-def" => Self::GameDef,
			"loader-def" => Self::LoaderDef,
			"runtime-def" => Self::RuntimeDef,
			_ => return None,
		})
	}
}

pub fn domain_tag(kind: ObjectKind) -> Vec<u8> {
	let mut tag = Vec::with_capacity(12 + kind.as_str().len() + 1);
	tag.extend_from_slice(b"GAMEDIST/v1/");
	tag.extend_from_slice(kind.as_str().as_bytes());
	tag.push(0x00);
	tag
}

pub fn object_id(kind: ObjectKind, payload: &[u8]) -> [u8; 32] {
	let mut hasher = Sha256::new();
	hasher.update(domain_tag(kind));
	hasher.update(payload);
	hasher.finalize().into()
}

pub fn object_id_string(kind: ObjectKind, payload: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(object_id(kind, payload)))
}
