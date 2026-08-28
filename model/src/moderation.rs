pub const REASON_TAXONOMY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonCode {
	PolicyDisallowed,
	MalwareSuspected,
	MalwareConfirmed,
	Impersonation,
	TrademarkClaim,
	RightsComplaint,
	Spam,
	ForkDetected,
	Broken,
	AuthorRequest,
	LegalOrder,
	AdultContent,
}

impl ReasonCode {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::PolicyDisallowed => "policy-disallowed",
			Self::MalwareSuspected => "malware-suspected",
			Self::MalwareConfirmed => "malware-confirmed",
			Self::Impersonation => "impersonation",
			Self::TrademarkClaim => "trademark-claim",
			Self::RightsComplaint => "rights-complaint",
			Self::Spam => "spam",
			Self::ForkDetected => "fork-detected",
			Self::Broken => "broken",
			Self::AuthorRequest => "author-request",
			Self::LegalOrder => "legal-order",
			Self::AdultContent => "adult-content",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"policy-disallowed" => Self::PolicyDisallowed,
			"malware-suspected" => Self::MalwareSuspected,
			"malware-confirmed" => Self::MalwareConfirmed,
			"impersonation" => Self::Impersonation,
			"trademark-claim" => Self::TrademarkClaim,
			"rights-complaint" => Self::RightsComplaint,
			"spam" => Self::Spam,
			"fork-detected" => Self::ForkDetected,
			"broken" => Self::Broken,
			"author-request" => Self::AuthorRequest,
			"legal-order" => Self::LegalOrder,
			"adult-content" => Self::AdultContent,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
	Known(ReasonCode),
	Unknown,
}

pub fn classify_reason(code: &str, taxonomy_version: u32) -> Reason {
	if taxonomy_version != REASON_TAXONOMY_VERSION {
		return Reason::Unknown;
	}
	match ReasonCode::parse(code) {
		Some(reason) => Reason::Known(reason),
		None => Reason::Unknown,
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
	Instance,
	Project,
	ReleaseDigest,
	Account,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeRef {
	pub kind: ScopeKind,
	pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanctionKind {
	Warning,
	UploadRestriction,
	Suspension,
}

impl SanctionKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Warning => "warning",
			Self::UploadRestriction => "upload-restriction",
			Self::Suspension => "suspension",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"warning" => Self::Warning,
			"upload-restriction" => Self::UploadRestriction,
			"suspension" => Self::Suspension,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanction {
	pub subject_user_id: String,
	pub org_id: Option<String>,
	pub kind: SanctionKind,
	pub reason_code: String,
	pub reason_taxonomy_version: u32,
	pub scope: ScopeRef,
	pub starts_at: i64,
	pub expires_at: Option<i64>,
	pub decided_by: String,
}

impl Sanction {
	pub fn is_active_at(&self, now: i64) -> bool {
		self.starts_at <= now && self.expires_at.is_none_or(|expires| now < expires)
	}

	pub fn reason(&self) -> Reason {
		classify_reason(&self.reason_code, self.reason_taxonomy_version)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportStatus {
	Open,
	AwaitingResponse,
	Upheld,
	Rejected,
	Withdrawn,
}

impl ReportStatus {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Open => "open",
			Self::AwaitingResponse => "awaiting-response",
			Self::Upheld => "upheld",
			Self::Rejected => "rejected",
			Self::Withdrawn => "withdrawn",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"open" => Self::Open,
			"awaiting-response" => Self::AwaitingResponse,
			"upheld" => Self::Upheld,
			"rejected" => Self::Rejected,
			"withdrawn" => Self::Withdrawn,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpersonationReport {
	pub claim_kind: String,
	pub claimant_ref: String,
	pub target_project_id: Option<String>,
	pub target_handle: Option<String>,
	pub evidence_ref: String,
	pub status: ReportStatus,
	pub decided_at: Option<i64>,
}

impl ImpersonationReport {
	pub fn validate(&self) -> Result<(), String> {
		if self.claim_kind.trim().is_empty() {
			return Err("claim_kind is required".to_string());
		}
		if self.claimant_ref.trim().is_empty() {
			return Err("claimant_ref is required".to_string());
		}
		if self.evidence_ref.trim().is_empty() {
			return Err("evidence_ref is required".to_string());
		}
		let project = self
			.target_project_id
			.as_deref()
			.is_some_and(|value| !value.trim().is_empty());
		let handle = self.target_handle.as_deref().is_some_and(|value| !value.trim().is_empty());
		if !project && !handle {
			return Err("a target project or handle is required".to_string());
		}
		if let Some(handle) = &self.target_handle
			&& !valid_handle(handle)
		{
			return Err(format!("`{handle}` is not a valid handle"));
		}
		if self.status != ReportStatus::Open && self.decided_at.is_none() {
			return Err("a decided report needs decided_at".to_string());
		}
		Ok(())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalRequestKind {
	CopyrightNotice,
	CounterNotice,
	CourtOrder,
	LawEnforcementRequest,
	PlatformPolicyAction,
	Other,
}

impl LegalRequestKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::CopyrightNotice => "copyright-notice",
			Self::CounterNotice => "counter-notice",
			Self::CourtOrder => "court-order",
			Self::LawEnforcementRequest => "law-enforcement-request",
			Self::PlatformPolicyAction => "platform-policy-action",
			Self::Other => "other",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"copyright-notice" => Self::CopyrightNotice,
			"counter-notice" => Self::CounterNotice,
			"court-order" => Self::CourtOrder,
			"law-enforcement-request" => Self::LawEnforcementRequest,
			"platform-policy-action" => Self::PlatformPolicyAction,
			"other" => Self::Other,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalTarget {
	Project,
	Release,
}

impl LegalTarget {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Project => "project",
			Self::Release => "release",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"project" => Self::Project,
			"release" => Self::Release,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalAction {
	None,
	Noted,
	AvailabilityDisabled,
	AvailabilityRestored,
}

impl LegalAction {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::None => "none",
			Self::Noted => "noted",
			Self::AvailabilityDisabled => "availability-disabled",
			Self::AvailabilityRestored => "availability-restored",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"none" => Self::None,
			"noted" => Self::Noted,
			"availability-disabled" => Self::AvailabilityDisabled,
			"availability-restored" => Self::AvailabilityRestored,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalRequest {
	pub kind: LegalRequestKind,
	pub claimant_ref: String,
	pub target_kind: LegalTarget,
	pub target_id: String,
	pub stated_basis: String,
	pub received_at: i64,
	pub action_taken: LegalAction,
	pub designated_agent_ref: Option<String>,
	pub responds_to: Option<String>,
}

impl LegalRequest {
	pub fn validate(&self) -> Result<(), String> {
		if self.claimant_ref.trim().is_empty() {
			return Err("claimant_ref is required".to_string());
		}
		if self.target_id.trim().is_empty() {
			return Err("target_id is required".to_string());
		}
		if self.stated_basis.trim().is_empty() {
			return Err("stated_basis is required".to_string());
		}
		Ok(())
	}
}

pub fn valid_handle(handle: &str) -> bool {
	!handle.is_empty()
		&& handle.len() <= 64
		&& handle.chars().all(|character| {
			character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-' || character == '_'
		})
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleDecision {
	Grant,
	Collision,
	Dispute,
}

pub fn decide_handle_claim(taken: bool, protected: bool) -> HandleDecision {
	if protected {
		HandleDecision::Dispute
	} else if taken {
		HandleDecision::Collision
	} else {
		HandleDecision::Grant
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reason_codes_are_versioned() {
		assert_eq!(classify_reason("spam", 1), Reason::Known(ReasonCode::Spam));
		assert_eq!(classify_reason("spam", 2), Reason::Unknown);
		assert_eq!(classify_reason("not-a-code", 1), Reason::Unknown);
	}

	#[test]
	fn an_impersonation_report_needs_a_target_and_a_basis() {
		let report = ImpersonationReport {
			claim_kind: "trademark".to_string(),
			claimant_ref: "holder@example.org".to_string(),
			target_project_id: None,
			target_handle: Some("official-brand".to_string()),
			evidence_ref: "https://example.org/evidence".to_string(),
			status: ReportStatus::Open,
			decided_at: None,
		};
		assert!(report.validate().is_ok());

		let mut untargeted = report.clone();
		untargeted.target_handle = None;
		assert!(untargeted.validate().is_err());

		let mut undecided = report.clone();
		undecided.status = ReportStatus::Upheld;
		assert!(undecided.validate().is_err());

		let mut bad_handle = report;
		bad_handle.target_handle = Some("Not A Handle".to_string());
		assert!(bad_handle.validate().is_err());
	}

	#[test]
	fn a_legal_request_needs_a_claimant_a_target_and_a_basis() {
		let request = LegalRequest {
			kind: LegalRequestKind::CopyrightNotice,
			claimant_ref: "holder@example.org".to_string(),
			target_kind: LegalTarget::Project,
			target_id: "gd:sha256:aa".to_string(),
			stated_basis: "reproduces a copyrighted asset".to_string(),
			received_at: 1_760_000_000,
			action_taken: LegalAction::AvailabilityDisabled,
			designated_agent_ref: None,
			responds_to: None,
		};
		assert!(request.validate().is_ok());

		let mut incomplete = request.clone();
		incomplete.stated_basis = "  ".to_string();
		assert!(incomplete.validate().is_err());

		assert_eq!(LegalRequestKind::parse("court-order"), Some(LegalRequestKind::CourtOrder));
		assert_eq!(
			LegalAction::parse("availability-restored"),
			Some(LegalAction::AvailabilityRestored)
		);
		assert_eq!(LegalTarget::parse("release"), Some(LegalTarget::Release));
		assert_eq!(LegalRequestKind::parse("subpoena"), None);
	}

	#[test]
	fn sanctions_expire() {
		let sanction = Sanction {
			subject_user_id: "u".to_string(),
			org_id: None,
			kind: SanctionKind::UploadRestriction,
			reason_code: "spam".to_string(),
			reason_taxonomy_version: 1,
			scope: ScopeRef {
				kind: ScopeKind::Project,
				id: "p".to_string(),
			},
			starts_at: 100,
			expires_at: Some(200),
			decided_by: "admin".to_string(),
		};
		assert!(sanction.is_active_at(150));
		assert!(!sanction.is_active_at(200));
		assert!(!sanction.is_active_at(99));
	}

	#[test]
	fn handle_collisions_route_to_dispute() {
		assert_eq!(decide_handle_claim(false, false), HandleDecision::Grant);
		assert_eq!(decide_handle_claim(true, false), HandleDecision::Collision);
		assert_eq!(decide_handle_claim(true, true), HandleDecision::Dispute);
		assert!(valid_handle("cleanroom-mc"));
		assert!(!valid_handle("Cleanroom"));
	}
}
