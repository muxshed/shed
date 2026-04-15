<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { cn } from '$lib/utils';
	import type { Snippet } from 'svelte';

	let {
		class: className,
		children,
	}: {
		class?: string;
		children?: Snippet;
	} = $props();
</script>

<div class={cn('w-full max-w-lg rounded-lg border border-neutral-700 bg-neutral-900 p-6 shadow-xl', className)}>
	{#if children}{@render children()}{/if}
</div>
