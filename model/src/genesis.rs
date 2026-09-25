use moraine_codec::Value;
use moraine_crypto::{KeyId, ObjectKind, key_id};

use crate::canonical::{
	Canonical, Fields, expect_array, expect_bytes, expect_i64, expect_text, expect_text_array, expect_u32, map_of,
};
use crate::error::{ModelError, RejectReason};
use crate::signed::ALG_ED25519;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenesisKind {
	Project,
	Game,
	Loader,
	Runtime,
}

impl GenesisKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Project => "project",
			Self::Game => "game",
			Self::Loader => "loader",
			Self::Runtime => "runtime",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"project" => Self::Project,
			"game" => Self::Game,
			"loader" => Self::Loader,
			"runtime" => Self::Runtime,
			_ => return None,
		})
	}

	pub fn authority_kinds(self) -> &'static [ObjectKind] {
		match self {
			Self::Project => &[
				ObjectKind::Delegation,
				ObjectKind::Release,
				ObjectKind::Profile,
				ObjectKind::FeedEntry,
				ObjectKind::Changelog,
				ObjectKind::Modpack,
				ObjectKind::Attestation,
			],
			Self::Game => &[ObjectKind::Delegation, ObjectKind::GameDef],
			Self::Loader => &[ObjectKind::Delegation, ObjectKind::LoaderDef],
			Self::Runtime => &[ObjectKind::Delegation, ObjectKind::RuntimeDef],
		}
	}

	pub fn allows_authorized_kind(self, kind: &str) -> bool {
		ObjectKind::parse(kind).is_some_and(|kind| self.authority_kinds().contains(&kind))
	}

	pub fn validate_authorized_kinds(&self, kinds: &[String]) -> Result<(), ModelError> {
		for kind in kinds {
			let Some(parsed) = ObjectKind::parse(kind) else {
				return Err(ModelError::field(RejectReason::InvalidFieldValue, "authorized_kinds"));
			};
			if !self.authority_kinds().contains(&parsed) {
				return Err(ModelError::new(
					RejectReason::UnauthorizedKind,
					format!("{} genesis cannot authorize `{kind}`", self.as_str()),
				));
			}
		}
		for (index, kind) in kinds.iter().enumerate() {
			if kinds[..index].iter().any(|earlier| earlier == kind) {
				return Err(ModelError::field(RejectReason::InvalidFieldValue, "authorized_kinds"));
			}
		}
		Ok(())
	}

	const fn minimum_kinds(self) -> &'static [&'static str] {
		match self {
			Self::Project => &["delegation", "release", "profile"],
			Self::Game => &["delegation", "game-def"],
			Self::Loader => &["delegation", "loader-def"],
			Self::Runtime => &["delegation", "runtime-def"],
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootKey {
	pub key_id: KeyId,
	pub public_key: Vec<u8>,
}

impl RootKey {
	pub fn from_public_key(public_key: Vec<u8>) -> Result<Self, ModelError> {
		let id = key_id(ALG_ED25519, &public_key)
			.map_err(|_| ModelError::new(RejectReason::InvalidFieldValue, "invalid root public key"))?;
		Ok(Self { key_id: id, public_key })
	}
}

impl Canonical for RootKey {
	fn to_value(&self) -> Value {
		map_of(
			"RootKey",
			[
				("key_id", Value::bytes(self.key_id.digest())),
				("public_key", Value::bytes(self.public_key.clone())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("RootKey", value)?.reject_unknown(&["key_id", "public_key"])?;
		let digest: [u8; 32] = expect_bytes(fields.required("key_id")?, "key_id")?
			.try_into()
			.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "key_id"))?;
		let public_key = expect_bytes(fields.required("public_key")?, "public_key")?;
		Ok(Self {
			key_id: KeyId::from_digest(ALG_ED25519, digest),
			public_key,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genesis {
	pub protocol: u32,
	pub kind: GenesisKind,
	pub nonce: Vec<u8>,
	pub roots: Vec<RootKey>,
	pub threshold: u32,
	pub authorized_kinds: Vec<String>,
	pub home_hint: Option<String>,
	pub contacts: Option<Vec<String>>,
	pub created_at: i64,
}

impl Genesis {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.nonce.len() < 16 {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"nonce must be at least 16 bytes",
			));
		}
		if self.roots.is_empty() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"genesis must list at least one root key",
			));
		}
		for root in &self.roots {
			let derived = key_id(ALG_ED25519, &root.public_key)
				.map_err(|_| ModelError::new(RejectReason::InvalidFieldValue, "invalid root public key"))?;
			if derived != root.key_id {
				return Err(ModelError::new(
					RejectReason::InvalidFieldValue,
					"root key_id does not match its public key",
				));
			}
		}
		for (index, root) in self.roots.iter().enumerate() {
			if self.roots[..index].iter().any(|earlier| earlier.key_id == root.key_id) {
				return Err(ModelError::new(RejectReason::DuplicateKey, "duplicate root key id"));
			}
		}
		if self.threshold == 0 || usize::try_from(self.threshold).unwrap_or(usize::MAX) > self.roots.len() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"threshold must be between 1 and the number of roots",
			));
		}
		self.kind.validate_authorized_kinds(&self.authorized_kinds)?;
		for required in self.kind.minimum_kinds() {
			if !self.authorized_kinds.iter().any(|kind| kind == required) {
				return Err(ModelError::new(
					RejectReason::GenesisKindRequirement,
					format!("{} genesis must authorize `{required}`", self.kind.as_str()),
				));
			}
		}
		Ok(())
	}
}

impl Canonical for Genesis {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("kind", Value::text(self.kind.as_str())),
			("nonce", Value::bytes(self.nonce.clone())),
			(
				"roots",
				Value::array(self.roots.iter().map(Canonical::to_value).collect::<Vec<_>>()),
			),
			("threshold", Value::int(i64::from(self.threshold))),
			(
				"authorized_kinds",
				Value::array(self.authorized_kinds.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			),
		];
		if let Some(home_hint) = &self.home_hint {
			pairs.push(("home_hint", Value::text(home_hint.clone())));
		}
		if let Some(contacts) = &self.contacts {
			pairs.push((
				"contacts",
				Value::array(contacts.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		pairs.push(("created_at", Value::int(self.created_at)));
		map_of("Genesis", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Genesis", value)?.reject_unknown(&[
			"protocol",
			"kind",
			"nonce",
			"roots",
			"threshold",
			"authorized_kinds",
			"home_hint",
			"contacts",
			"created_at",
		])?;
		let kind_text = expect_text(fields.required("kind")?, "kind")?;
		let kind =
			GenesisKind::parse(&kind_text).ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "kind"))?;
		let roots = expect_array(fields.required("roots")?, "roots")?
			.iter()
			.cloned()
			.map(RootKey::from_value)
			.collect::<Result<Vec<_>, _>>()?;
		let contacts = fields
			.optional("contacts")
			.map(|value| expect_text_array(value, "contacts"))
			.transpose()?;
		let genesis = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			kind,
			nonce: expect_bytes(fields.required("nonce")?, "nonce")?,
			roots,
			threshold: expect_u32(fields.required("threshold")?, "threshold")?,
			authorized_kinds: expect_text_array(fields.required("authorized_kinds")?, "authorized_kinds")?,
			home_hint: fields
				.optional("home_hint")
				.map(|value| expect_text(value, "home_hint"))
				.transpose()?,
			contacts,
			created_at: expect_i64(fields.required("created_at")?, "created_at")?,
		};
		genesis.validate()?;
		Ok(genesis)
	}
}
