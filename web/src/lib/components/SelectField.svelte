<script lang="ts">
	import { Check, ChevronDown } from '@lucide/svelte';
	import { Select } from 'bits-ui';

	export type SelectOption = { value: string; label: string };

	let {
		label,
		value = $bindable(''),
		options,
		placeholder = 'Choose…',
		disabled = false,
		hint = '',
		bare = false,
		onchange,
	}: {
		label: string;
		value?: string;
		options: SelectOption[];
		placeholder?: string;
		disabled?: boolean;
		hint?: string;
		bare?: boolean;
		onchange?: (value: string) => void;
	} = $props();
</script>

{#snippet control()}
	<Select.Root
		type="single"
		bind:value
		{disabled}
		items={options}
		onValueChange={(next) => onchange?.(next)}
	>
		<Select.Trigger
			class="input flex w-full items-center justify-between gap-2 text-left data-[disabled]:opacity-50"
		>
			<Select.Value {placeholder} />
			<ChevronDown size={16} class="shrink-0 opacity-60" />
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
										<Check size={16} class="text-primary" />
									{/if}
								</div>
							{/snippet}
						</Select.Item>
					{/each}
				</Select.Viewport>
			</Select.Content>
		</Select.Portal>
	</Select.Root>
{/snippet}

{#if bare}
	{@render control()}
{:else}
	<fieldset class="fieldset">
		<legend class="fieldset-legend">{label}</legend>
		{@render control()}
		{#if hint}<p class="label">{hint}</p>{/if}
	</fieldset>
{/if}
