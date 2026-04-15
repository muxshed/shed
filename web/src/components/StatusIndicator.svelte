<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import type { PipelineState } from '$lib/types';

	let { state }: { state: PipelineState } = $props();

	function color(): string {
		switch (state.state) {
			case 'live': return 'bg-red-500';
			case 'starting':
			case 'stopping': return 'bg-yellow-500 animate-pulse';
			case 'error': return 'bg-red-700';
			default: return 'bg-neutral-600';
		}
	}

	function label(): string {
		if (state.state === 'live' && 'started_at' in state) {
			return 'LIVE';
		}
		return state.state.toUpperCase();
	}
</script>

<div class="flex items-center gap-2">
	<span class="h-3 w-3 rounded-full {color()}"></span>
	<span class="text-sm font-semibold tracking-wide">{label()}</span>
</div>
