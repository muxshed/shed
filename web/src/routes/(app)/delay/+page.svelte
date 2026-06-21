<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import type { DelayConfig } from '$lib/types';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { notify } from '$lib/notify';

	let config = $state<DelayConfig>({ enabled: false, duration_ms: 7000, whisper_enabled: false });
	let saving = $state(false);

	onMount(async () => {
		config = await api.getDelay();
	});

	async function save() {
		saving = true;
		try {
			config = await api.updateDelay({
				enabled: config.enabled,
				duration_ms: config.duration_ms,
				whisper_enabled: config.whisper_enabled,
			});
		} catch (e) {
			notify.error(e);
		} finally {
			saving = false;
		}
	}

	async function bleep() {
		try {
			await api.triggerBleep();
		} catch (e) {
			notify.error(e);
		}
	}

	// Segmented buffer meter: 14 segments mapped across the 1–30s delay range.
	const SEGMENTS = 14;
	let filledSegments = $derived(
		config.enabled
			? Math.max(1, Math.round((config.duration_ms / 30000) * SEGMENTS))
			: 0,
	);
</script>

<div class="mx-auto max-w-2xl space-y-4">
	<section class="panel">
		<header class="panel__head">▮ Broadcast Delay</header>
		<div class="panel__body space-y-4">
			<div class="row">
				<span class="label">Delay state</span>
				<button
					onclick={() => { config.enabled = !config.enabled; save(); }}
					class="btn {config.enabled ? 'btn--go' : 'btn--ghost'}"
				>
					{config.enabled ? '● Enabled' : '○ Disabled'}
				</button>
			</div>

			<!-- Buffer readout -->
			<div class="row items-center">
				<span class="label">Buffer</span>
				<span class="led text-[28px]">{(config.duration_ms / 1000).toFixed(1)}s</span>
			</div>

			<!-- Segmented buffer meter -->
			<div class="flex gap-[2px]" aria-hidden="true">
				{#each Array(SEGMENTS) as _, i}
					<div
						class="h-3 flex-1 rounded-sm border"
						style="
							background: {i < filledSegments
								? i < SEGMENTS - 3
									? 'var(--color-amber)'
									: 'var(--color-amber-dim)'
								: 'transparent'};
							border-color: {i < filledSegments ? 'transparent' : 'var(--color-border-dim)'};
						"
					></div>
				{/each}
			</div>

			<div>
				<label class="field-label" for="delay-ms">Delay Duration (ms)</label>
				<input
					id="delay-ms"
					bind:value={config.duration_ms}
					type="number"
					min="1000"
					max="30000"
					step="1000"
					class="input"
				/>
			</div>

			<button onclick={() => (config.whisper_enabled = !config.whisper_enabled)} class="flex items-center gap-3 cursor-pointer">
				<Checkbox checked={config.whisper_enabled} />
				<span class="text-amber-dim">Whisper auto-detect (experimental)</span>
			</button>

			<button onclick={save} disabled={saving} class="btn">
				{saving ? 'Saving…' : 'Save'}
			</button>
		</div>
	</section>

	<section class="panel">
		<header class="panel__head">▮ Manual Bleep</header>
		<div class="panel__body space-y-3">
			<p class="text-amber-dim text-xs leading-relaxed">
				Triggers a 1-second bleep tone, muting audio in the delay buffer.
			</p>
			<button onclick={bleep} class="btn btn--danger">▲ Bleep</button>
		</div>
	</section>
</div>
