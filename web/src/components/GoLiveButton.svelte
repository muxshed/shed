<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { api } from '$lib/api';
	import { pipelineState, isLive, isTransitioning } from '$lib/stores/pipeline';

	let loading = $state(false);
	let error = $state('');

	async function toggle() {
		loading = true;
		error = '';
		try {
			if ($isLive) {
				await api.stopStream();
			} else {
				await api.startStream();
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			loading = false;
		}
	}

	function buttonClass(): string {
		if ($isLive) return 'bg-red-600 hover:bg-red-700';
		if ($isTransitioning) return 'bg-yellow-600 cursor-not-allowed';
		return 'bg-green-600 hover:bg-green-700';
	}

	function buttonLabel(): string {
		if ($pipelineState.state === 'starting') return 'Starting...';
		if ($pipelineState.state === 'stopping') return 'Stopping...';
		if ($isLive) return 'End Stream';
		return 'Go Live';
	}
</script>

<div>
	<button
		onclick={toggle}
		disabled={$isTransitioning || loading}
		class="rounded-lg px-8 py-3 text-lg font-bold text-white transition-colors {buttonClass()} disabled:opacity-50"
	>
		{buttonLabel()}
	</button>
	{#if error}
		<p class="mt-2 text-sm text-red-400">{error}</p>
	{/if}
</div>
