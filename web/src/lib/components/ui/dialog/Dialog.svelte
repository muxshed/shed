<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		open = $bindable(false),
		children,
	}: {
		open?: boolean;
		children?: Snippet;
	} = $props();
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center">
		<button class="fixed inset-0 bg-black/60" onclick={() => (open = false)}></button>
		<div class="relative z-50">
			{#if children}{@render children()}{/if}
		</div>
	</div>
{/if}
