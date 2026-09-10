<script lang="ts">
	import { Select } from 'bits-ui';

	export type SelectOption = { value: string; label: string };

	let {
		label,
		value = $bindable<string[]>([]),
		options,
		placeholder = 'Any',
		disabled = false,
		hint = '',
		onchange,
	}: {
		label: string;
		value?: string[];
		options: SelectOption[];
		placeholder?: string;
		disabled?: boolean;
		hint?: string;
		onchange?: (value: string[]) => void;
	} = $props();
</script>

<fieldset class="fieldset">
	<legend class="fieldset-legend">{label}</legend>
	<Select.Root
		type="multiple"
		bind:value
		{disabled}
		items={options}
		onValueChange={(next) => onchange?.(next)}
	>
		<Select.Trigger
			class="input flex h-auto min-h-10 w-full items-center justify-between gap-2 py-1.5 text-left data-[disabled]:opacity-50"
		>
			<Select.Value>
				{#snippet child({ selection, placeholder: empty })}
					{#if selection.type === 'multiple' && selection.selected.length > 0}
						<span class="flex flex-wrap gap-1">
							{#each selection.selected as item (item.value)}
								<span class="badge badge-sm">{item.label}</span>
							{/each}
						</span>
					{:else}
						<span class="opacity-50">{empty}</span>
					{/if}
				{/snippet}
			</Select.Value>
			<svg
				class="h-4 w-4 shrink-0 opacity-60"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				stroke-linecap="round"
				aria-hidden="true"
			>
				<path d="m6 9 6 6 6-6" />
			</svg>
		</Select.Trigger>
		<Select.Portal>
			<Select.Content
				class="z-50 max-h-72 w-[var(--bits-select-anchor-width)] overflow-hidden rounded-box border border-base-300 bg-base-100 shadow-xl"
				sideOffset={4}
			>
				<Select.Viewport class="max-h-72 overflow-y-auto p-1">
					{#each options as option (option.value)}
						<Select.Item value={option.value} label={option.label}>
							{#snippet child({ props, selected })}
								<div
									{...props}
									class="flex cursor-pointer items-center justify-between gap-2 rounded-field px-3 py-2 text-sm data-[highlighted]:bg-base-200"
								>
									<span class="truncate">{option.label}</span>
									{#if selected}
										<svg
											class="h-4 w-4 text-primary"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											stroke-width="2.5"
											stroke-linecap="round"
											stroke-linejoin="round"
											aria-hidden="true"
										>
											<path d="m5 13 4 4L19 7" />
										</svg>
									{/if}
								</div>
							{/snippet}
						</Select.Item>
					{/each}
				</Select.Viewport>
			</Select.Content>
		</Select.Portal>
	</Select.Root>
	{#if hint}<p class="label">{hint}</p>{/if}
</fieldset>
