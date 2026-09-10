<script lang="ts">
	let { name, id = '', size = 40 }: { name: string; id?: string; size?: number } = $props();

	const initials = $derived(
		name
			.trim()
			.split(/\s+/)
			.slice(0, 2)
			.map((word) => word.charAt(0).toUpperCase())
			.join('') || '?',
	);

	const hue = $derived.by(() => {
		let value = 0;
		for (const character of id || name) {
			value = (value * 31 + character.charCodeAt(0)) % 360;
		}
		return value;
	});
</script>

<div class="avatar avatar-placeholder" style={`width:${size}px;height:${size}px`}>
	<div
		class="rounded-box font-semibold text-white"
		style={`background:hsl(${hue} 55% 42%);font-size:${Math.round(size * 0.4)}px`}
	>
		<span>{initials}</span>
	</div>
</div>
