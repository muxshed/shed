<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { Checkbox as CheckboxPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils';

	let {
		checked = $bindable(false),
		class: className,
		...restProps
	}: CheckboxPrimitive.RootProps & { class?: string } = $props();
</script>

<CheckboxPrimitive.Root
	bind:checked
	class={cn(
		'peer inline-flex h-4 w-4 shrink-0 items-center justify-center rounded border border-neutral-600 bg-neutral-800',
		'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-blue-500',
		'disabled:cursor-not-allowed disabled:opacity-50',
		'data-[state=checked]:border-blue-500 data-[state=checked]:bg-blue-600',
		'transition-colors cursor-pointer',
		className,
	)}
	{...restProps}
>
	{#snippet children({ checked: isChecked })}
		{#if isChecked}
			<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" class="text-white">
				<polyline points="20 6 9 17 4 12"></polyline>
			</svg>
		{/if}
	{/snippet}
</CheckboxPrimitive.Root>
