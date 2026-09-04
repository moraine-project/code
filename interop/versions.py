SCHEMES = {"exact", "set", "semver", "ordered-list", "calendar", "any"}
ORDERINGS = {"semver", "ordered-list", "calendar", "opaque"}

SATISFIED = "satisfied"
NOT_SATISFIED = "not-satisfied"
UNKNOWN = "unknown"


def evaluate(ordering, catalog, scheme, values, version):
    if ordering not in ORDERINGS:
        return None
    if scheme not in SCHEMES:
        return UNKNOWN
    if scheme == "any":
        return SATISFIED
    if scheme in ("exact", "set"):
        return SATISFIED if version in values else NOT_SATISFIED
    if scheme == "semver":
        if ordering != "semver":
            return UNKNOWN
        for comparator in values:
            result = _satisfies(comparator, version)
            if result is None:
                return UNKNOWN
            if not result:
                return NOT_SATISFIED
        return SATISFIED
    if scheme == "ordered-list":
        if ordering != "ordered-list":
            return UNKNOWN
        return _ranges(catalog, values, version)
    if scheme == "calendar":
        if ordering != "calendar":
            return UNKNOWN
        return _ranges(catalog, values, version)
    return UNKNOWN


def _ranges(catalog, values, version):
    try:
        position = catalog.index(version)
    except ValueError:
        return UNKNOWN
    for value in values:
        if _range_contains(catalog, value, position):
            return SATISFIED
    return NOT_SATISFIED


def _range_contains(catalog, value, position):
    if "..=" in value:
        start_text, end_text = value.split("..=", 1)
        inclusive = True
    elif ".." in value:
        start_text, end_text = value.split("..", 1)
        inclusive = False
    else:
        return catalog[position] == value
    try:
        start = catalog.index(start_text)
        end = catalog.index(end_text)
    except ValueError:
        return False
    after_start = position >= start
    before_end = position <= end if inclusive else position < end
    return after_start and before_end


class _Semver:
    def __init__(self, major, minor, patch, pre, components):
        self.major = major
        self.minor = minor
        self.patch = patch
        self.pre = pre
        self.components = components


def _parse_semver(value):
    core_and_build = value.split("+", 1)
    core_text = core_and_build[0]
    if "-" in core_text:
        core, pre_text = core_text.split("-", 1)
    else:
        core, pre_text = core_text, None
    parts = core.split(".")
    if len(parts) > 3:
        return None
    try:
        major = int(parts[0])
        minor = int(parts[1]) if len(parts) > 1 and parts[1] != "" else 0
        patch = int(parts[2]) if len(parts) > 2 and parts[2] != "" else 0
    except ValueError:
        return None
    components = len([part for part in parts if part != ""])
    if components > 3:
        components = 3
    pre = None
    if pre_text:
        pre = _parse_prerelease(pre_text)
        if pre is None:
            return None
    return _Semver(major, minor, patch, pre, components)


def _parse_prerelease(text):
    identifiers = []
    for identifier in text.split("."):
        if identifier == "":
            return None
        identifiers.append(int(identifier) if identifier.isdigit() else identifier)
    return identifiers


def _compare_pre(left, right):
    for a, b in zip(left, right):
        if isinstance(a, int) and isinstance(b, int):
            if a != b:
                return -1 if a < b else 1
        elif isinstance(a, int):
            return -1
        elif isinstance(b, int):
            return 1
        elif a != b:
            return -1 if a < b else 1
    if len(left) != len(right):
        return -1 if len(left) < len(right) else 1
    return 0


def _compare(left, right):
    for a, b in (
        (left.major, right.major),
        (left.minor, right.minor),
        (left.patch, right.patch),
    ):
        if a != b:
            return -1 if a < b else 1
    if left.pre is None and right.pre is None:
        return 0
    if left.pre is None:
        return 1
    if right.pre is None:
        return -1
    return _compare_pre(left.pre, right.pre)


def _satisfies(comparator, version):
    for operator in ("<=", ">=", "<", ">", "="):
        if comparator.startswith(operator):
            bound_text = comparator[len(operator) :]
            break
    else:
        operator, bound_text = "", comparator
    target = _parse_semver(version)
    bound = _parse_semver(bound_text)
    if target is None or bound is None:
        return None
    if operator == "" and bound.components < 3:
        return _has_prefix(target, bound, bound.components)
    ordering = _compare(target, bound)
    if operator in ("", "="):
        return ordering == 0
    if operator == "<":
        return ordering < 0
    if operator == "<=":
        return ordering <= 0
    if operator == ">":
        return ordering > 0
    return ordering >= 0


def _has_prefix(target, bound, components):
    if target.major != bound.major:
        return False
    if components >= 2 and target.minor != bound.minor:
        return False
    if components >= 3 and target.patch != bound.patch:
        return False
    return True
