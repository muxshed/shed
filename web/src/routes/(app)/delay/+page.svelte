<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import type { DelayConfig } from '$lib/types';
	import { Checkbox } from '$lib/components/ui/checkbox';

	let config = $state<DelayConfig>({ enabled: false, duration_ms: 7000, whisper_enabled: false });
	let saving = $state(false);
	let error = $state('');

	onMount(async () => {
		config = await api.getDelay();
	});

	async function save() {
		saving = true;
		error = '';
		try {
			config = await api.updateDelay({
				enabled: config.enabled,
				duration_ms: config.duration_ms,
				whisper_enabled: config.whisper_enabled,
			});
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to update delay';
		} finally {
			saving = false;
		}
	}

	async function bleep() {
		try {
			await api.triggerBleep();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Bleep failed';
		}
	}
</script>

<div class="mx-auto max-w-2xl">
	<h1 class="mb-6 text-2xl font-bold">Broadcast Delay</h1>

	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<div class="mb-4 flex items-center justify-between">
			<h2 class="text-sm font-semibold text-neutral-400">Delay Configuration</h2>
			<button
				onclick={() => { config.enabled = !config.enabled; save(); }}
				class="rounded px-3 py-1 text-xs font-medium {config.enabled
					? 'bg-green-900 text-green-400'
					: 'bg-neutral-700 text-neutral-400'} hover:opacity-80"
			>
				{config.enabled ? 'Enabled' : 'Disabled'}
			</button>
		</div>

		<div class="mb-4">
			<label class="mb-1 block text-xs text-neutral-400">Delay Duration (ms)</label>
			<input
				bind:value={config.duration_ms}
				type="number"
				min="1000"
				max="30000"
				step="1000"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<p class="mt-1 text-xs text-neutral-500">{(config.duration_ms / 1000).toFixed(1)} seconds</p>
		</div>

		<button onclick={() => (config.whisper_enabled = !config.whisper_enabled)} class="mb-4 flex items-center gap-3 cursor-pointer">
			<Checkbox checked={config.whisper_enabled} />
			<span class="text-sm text-neutral-300">Whisper auto-detect (experimental)</span>
		</button>

		<button
			onclick={save}
			disabled={saving}
			class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
		>
			{saving ? 'Saving...' : 'Save'}
		</button>

		{#if error}
			<p class="mt-2 text-sm text-red-400">{error}</p>
		{/if}
	</div>

	<div class="rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Manual Bleep</h2>
		<p class="mb-3 text-xs text-neutral-500">
			Triggers a 1-second bleep tone, muting audio in the delay buffer.
		</p>
		<button
			onclick={bleep}
			class="rounded bg-red-700 px-6 py-3 text-sm font-bold text-white hover:bg-red-600"
		>
			BLEEP
		</button>
	</div>
</div>
