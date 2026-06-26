<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
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
		'peer inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-sm border border-border bg-well',
		'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-amber',
		'disabled:cursor-not-allowed disabled:opacity-50',
		'data-[state=checked]:border-amber data-[state=checked]:bg-amber',
		'transition-colors cursor-pointer',
		className,
	)}
	{...restProps}
>
	{#snippet children({ checked: isChecked })}
		{#if isChecked}
			<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" class="text-bg">
				<polyline points="20 6 9 17 4 12"></polyline>
			</svg>
		{/if}
	{/snippet}
</CheckboxPrimitive.Root>
