<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import type { PipelineState } from '$lib/types';

	let { state }: { state: PipelineState } = $props();

	function dotStyle(): string {
		switch (state.state) {
			case 'live': return 'background: var(--color-live-glow)';
			case 'starting':
			case 'stopping': return 'background: var(--color-warning)';
			case 'error': return 'background: var(--color-danger)';
			default: return 'background: var(--color-amber-muted)';
		}
	}

	function textClass(): string {
		switch (state.state) {
			case 'live': return 'text-live';
			case 'starting':
			case 'stopping': return 'text-warning';
			case 'error': return 'text-danger';
			default: return 'text-amber-dim';
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
	<span class="h-3 w-3 rounded-full" style={dotStyle()}></span>
	<span class="text-[13px] font-medium tracking-[1px] uppercase {textClass()}">{label()}</span>
</div>
