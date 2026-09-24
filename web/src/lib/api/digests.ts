export function digestHex(digest: string): string {
	return digest.startsWith('sha256:') ? digest.slice('sha256:'.length) : digest;
}

export function shortDigest(id: string, length = 12): string {
	const hex = id.startsWith('gd:sha256:') ? id.slice('gd:sha256:'.length) : id;
	if (hex.length <= length) {
		return hex;
	}
	return `${hex.slice(0, length)}…`;
}
