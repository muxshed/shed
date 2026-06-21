<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { api } from '$lib/api';
	import { pipelineState, isLive, isTransitioning } from '$lib/stores/pipeline';
	import { notify } from '$lib/notify';

	let loading = $state(false);

	async function toggle() {
		loading = true;
		try {
			if ($isLive) {
				await api.stopStream();
			} else {
				await api.startStream();
			}
		} catch (e) {
			notify.error(e);
		} finally {
			loading = false;
		}
	}

	function buttonClass(): string {
		if ($isLive) return 'btn--danger';
		return 'btn--go';
	}

	function buttonLabel(): string {
		if ($pipelineState.state === 'starting') return '▸ Starting…';
		if ($pipelineState.state === 'stopping') return '■ Stopping…';
		if ($isLive) return '■ End Stream';
		return '● Go Live';
	}
</script>

<div>
	<button
		onclick={toggle}
		disabled={$isTransitioning || loading}
		class="btn {buttonClass()} min-h-12 px-8 text-[15px]"
	>
		{buttonLabel()}
	</button>
</div>
