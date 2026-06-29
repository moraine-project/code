use std::cmp::Ordering;

use crate::compatibility::{Predicate, PredicateResult, Scheme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderingScheme {
	Semver,
	OrderedList,
	Calendar,
	Opaque,
}

impl OrderingScheme {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Semver => "semver",
			Self::OrderedList => "ordered-list",
			Self::Calendar => "calendar",
			Self::Opaque => "opaque",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"semver" => Self::Semver,
			"ordered-list" => Self::OrderedList,
			"calendar" => Self::Calendar,
			"opaque" => Self::Opaque,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionCatalog {
	scheme: OrderingScheme,
	ordered: Vec<String>,
}

impl VersionCatalog {
	pub fn new(scheme: OrderingScheme, ordered: Vec<String>) -> Self {
		Self { scheme, ordered }
	}

	pub const fn scheme(&self) -> OrderingScheme {
		self.scheme
	}

	pub fn ordered(&self) -> &[String] {
		&self.ordered
	}

	pub fn compare(&self, left: &str, right: &str) -> Option<Ordering> {
		if left == right {
			return Some(Ordering::Equal);
		}
		match self.scheme {
			OrderingScheme::Semver => semver_compare(left, right),
			OrderingScheme::OrderedList | OrderingScheme::Calendar => {
				let left = self.ordered.iter().position(|candidate| candidate == left)?;
				let right = self.ordered.iter().position(|candidate| candidate == right)?;
				Some(left.cmp(&right))
			}
			OrderingScheme::Opaque => None,
		}
	}

	pub fn evaluate(&self, predicate: &Predicate, version: &str) -> PredicateResult {
		let Some(scheme) = predicate.scheme() else {
			return PredicateResult::Unknown;
		};
		match scheme {
			Scheme::Any => PredicateResult::Satisfied,
			Scheme::Exact | Scheme::Set => {
				if predicate.values.iter().any(|candidate| candidate == version) {
					PredicateResult::Satisfied
				} else {
					PredicateResult::NotSatisfied
				}
			}
			Scheme::Semver => {
				if self.scheme != OrderingScheme::Semver {
					return PredicateResult::Unknown;
				}
				let mut satisfied = true;
				for comparator in &predicate.values {
					match semver_satisfies(comparator, version) {
						Some(true) => {}
						Some(false) => satisfied = false,
						None => return PredicateResult::Unknown,
					}
				}
				if satisfied {
					PredicateResult::Satisfied
				} else {
					PredicateResult::NotSatisfied
				}
			}
			Scheme::OrderedList => {
				if self.scheme != OrderingScheme::OrderedList {
					return PredicateResult::Unknown;
				}
				self.evaluate_ranges(&predicate.values, version)
			}
			Scheme::Calendar => {
				if self.scheme != OrderingScheme::Calendar {
					return PredicateResult::Unknown;
				}
				self.evaluate_ranges(&predicate.values, version)
			}
		}
	}

	fn evaluate_ranges(&self, values: &[String], version: &str) -> PredicateResult {
		let Ok(position) = self.position(version) else {
			return PredicateResult::Unknown;
		};
		for value in values {
			let range = parse_range(value);
			if self.range_contains(&range, position) {
				return PredicateResult::Satisfied;
			}
		}
		PredicateResult::NotSatisfied
	}

	fn position(&self, version: &str) -> Result<usize, ()> {
		self.ordered.iter().position(|candidate| candidate == version).ok_or(())
	}

	fn range_contains(&self, range: &Range, position: usize) -> bool {
		match range {
			Range::Exact(value) => self.ordered.get(position).map(String::as_str) == Some(*value),
			Range::Between {
				from,
				from_inclusive,
				to,
				to_inclusive,
			} => {
				let Some(start) = self.position(from).ok() else {
					return false;
				};
				let Some(end) = self.position(to).ok() else {
					return false;
				};
				let after_start = if *from_inclusive {
					position >= start
				} else {
					position > start
				};
				let before_end = if *to_inclusive { position <= end } else { position < end };
				after_start && before_end
			}
		}
	}
}

enum Range<'a> {
	Exact(&'a str),
	Between {
		from: &'a str,
		from_inclusive: bool,
		to: &'a str,
		to_inclusive: bool,
	},
}

fn parse_range(value: &str) -> Range<'_> {
	if let Some((from, to)) = value.split_once("..=") {
		return Range::Between {
			from,
			from_inclusive: true,
			to,
			to_inclusive: true,
		};
	}
	if let Some((from, to)) = value.split_once("..") {
		return Range::Between {
			from,
			from_inclusive: true,
			to,
			to_inclusive: false,
		};
	}
	Range::Exact(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Semver {
	major: u64,
	minor: u64,
	patch: u64,
	pre: Option<Vec<PreIdentifier>>,
	components: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreIdentifier {
	Numeric(u64),
	Text(String),
}

fn parse_semver(value: &str) -> Option<Semver> {
	let without_build = value.split_once('+').map(|(core, _build)| core).unwrap_or(value);
	let (core_text, pre_text) = without_build
		.split_once('-')
		.map(|(core, pre)| (core, Some(pre)))
		.unwrap_or((without_build, None));
	let mut parts = core_text.split('.');
	let major = parts.next()?.parse::<u64>().ok()?;
	let minor = match parts.next() {
		Some(part) => part.parse::<u64>().ok()?,
		None => 0,
	};
	let patch = match parts.next() {
		Some(part) => part.parse::<u64>().ok()?,
		None => 0,
	};
	if parts.next().is_some() {
		return None;
	}
	let components = count_components(core_text);
	let pre = match pre_text {
		Some(pre) if !pre.is_empty() => Some(parse_prerelease(pre)?),
		_ => None,
	};
	Some(Semver {
		major,
		minor,
		patch,
		pre,
		components,
	})
}

fn count_components(core: &str) -> u8 {
	core.split('.').filter(|part| !part.is_empty()).count().min(3) as u8
}

fn parse_prerelease(value: &str) -> Option<Vec<PreIdentifier>> {
	value
		.split('.')
		.map(|identifier| {
			if identifier.is_empty() {
				return None;
			}
			match identifier.parse::<u64>() {
				Ok(number) => Some(PreIdentifier::Numeric(number)),
				Err(_) => Some(PreIdentifier::Text(identifier.to_string())),
			}
		})
		.collect()
}

fn semver_compare(left: &str, right: &str) -> Option<Ordering> {
	let left = parse_semver(left)?;
	let right = parse_semver(right)?;
	for ordering in [
		left.major.cmp(&right.major),
		left.minor.cmp(&right.minor),
		left.patch.cmp(&right.patch),
	] {
		if ordering != Ordering::Equal {
			return Some(ordering);
		}
	}
	match (&left.pre, &right.pre) {
		(None, None) => Some(Ordering::Equal),
		(None, Some(_)) => Some(Ordering::Greater),
		(Some(_), None) => Some(Ordering::Less),
		(Some(left), Some(right)) => Some(compare_prerelease(left, right)),
	}
}

fn compare_prerelease(left: &[PreIdentifier], right: &[PreIdentifier]) -> Ordering {
	for (left, right) in left.iter().zip(right) {
		let ordering = match (left, right) {
			(PreIdentifier::Numeric(a), PreIdentifier::Numeric(b)) => a.cmp(b),
			(PreIdentifier::Numeric(_), PreIdentifier::Text(_)) => Ordering::Less,
			(PreIdentifier::Text(_), PreIdentifier::Numeric(_)) => Ordering::Greater,
			(PreIdentifier::Text(a), PreIdentifier::Text(b)) => a.cmp(b),
		};
		if ordering != Ordering::Equal {
			return ordering;
		}
	}
	left.len().cmp(&right.len())
}

fn semver_satisfies(comparator: &str, version: &str) -> Option<bool> {
	let (operator, bound) = split_operator(comparator);
	let target = parse_semver(version)?;
	let bound = parse_semver(bound)?;
	match operator {
		Operator::Bare if bound.components < 3 => Some(has_prefix(&target, &bound, bound.components)),
		Operator::Bare | Operator::Equal => Some(semver_compare_to(&target, &bound) == Ordering::Equal),
		Operator::Less => Some(semver_compare_to(&target, &bound) == Ordering::Less),
		Operator::LessOrEqual => Some(semver_compare_to(&target, &bound) != Ordering::Greater),
		Operator::Greater => Some(semver_compare_to(&target, &bound) == Ordering::Greater),
		Operator::GreaterOrEqual => Some(semver_compare_to(&target, &bound) != Ordering::Less),
	}
}

fn has_prefix(target: &Semver, bound: &Semver, components: u8) -> bool {
	if target.major != bound.major {
		return false;
	}
	if components >= 2 && target.minor != bound.minor {
		return false;
	}
	if components >= 3 && target.patch != bound.patch {
		return false;
	}
	true
}

fn semver_compare_to(left: &Semver, right: &Semver) -> Ordering {
	for ordering in [
		left.major.cmp(&right.major),
		left.minor.cmp(&right.minor),
		left.patch.cmp(&right.patch),
	] {
		if ordering != Ordering::Equal {
			return ordering;
		}
	}
	match (&left.pre, &right.pre) {
		(None, None) => Ordering::Equal,
		(None, Some(_)) => Ordering::Greater,
		(Some(_), None) => Ordering::Less,
		(Some(left), Some(right)) => compare_prerelease(left, right),
	}
}

enum Operator {
	Bare,
	Equal,
	Less,
	LessOrEqual,
	Greater,
	GreaterOrEqual,
}

fn split_operator(comparator: &str) -> (Operator, &str) {
	if let Some(rest) = comparator.strip_prefix("<=") {
		(Operator::LessOrEqual, rest)
	} else if let Some(rest) = comparator.strip_prefix(">=") {
		(Operator::GreaterOrEqual, rest)
	} else if let Some(rest) = comparator.strip_prefix('<') {
		(Operator::Less, rest)
	} else if let Some(rest) = comparator.strip_prefix('>') {
		(Operator::Greater, rest)
	} else if let Some(rest) = comparator.strip_prefix('=') {
		(Operator::Equal, rest)
	} else {
		(Operator::Bare, comparator)
	}
}
