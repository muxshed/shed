<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { popout } from '$lib/popout';

	let { section, width = 640, height = 480 }: { section: string; width?: number; height?: number } = $props();
</script>

<button
	onclick={() => popout(section, width, height)}
	class="rounded p-1 text-neutral-600 transition-colors hover:bg-neutral-800 hover:text-neutral-300"
	title="Pop out to separate window"
>
	<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
		<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
		<polyline points="15 3 21 3 21 9"/>
		<line x1="10" y1="14" x2="21" y2="3"/>
	</svg>
</button>
