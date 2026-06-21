<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { sources } from '$lib/stores/pipeline';
	import SourceCard from '../../../components/SourceCard.svelte';
	import { notify } from '$lib/notify';
	import type { SourceKind } from '$lib/types';

	let name = $state('');
	let protocol = $state<'rtmp' | 'srt'>('rtmp');
	let srtPassphrase = $state('');
	let creating = $state(false);

	onMount(refresh);

	async function refresh() {
		sources.set(await api.listSources());
	}

	async function create() {
		if (!name.trim()) return;
		creating = true;
		try {
			let kind: SourceKind;
			if (protocol === 'srt') {
				kind = {
					type: 'srt',
					port: 0,
					passphrase: srtPassphrase.trim() || undefined,
				};
			} else {
				kind = { type: 'rtmp', stream_key: '' };
			}
			await api.createSource(name.trim(), kind);
			name = '';
			srtPassphrase = '';
			await refresh();
			notify.success('Source added');
		} catch (e) {
			notify.error(e);
		} finally {
			creating = false;
		}
	}
</script>

<div class="mx-auto max-w-4xl space-y-6">
	<section class="panel">
		<header class="panel__head">▮ Add Source</header>
		<div class="panel__body">
			<form onsubmit={(e) => { e.preventDefault(); create(); }} class="space-y-3">
				<div class="flex gap-3">
					<div>
						<label class="field-label" for="source-protocol">Protocol</label>
						<select id="source-protocol" bind:value={protocol} class="select">
							<option value="rtmp">RTMP</option>
							<option value="srt">SRT</option>
						</select>
					</div>
					<div class="flex-1">
						<label class="field-label" for="source-name">Source Name</label>
						<input
							id="source-name"
							bind:value={name}
							placeholder="e.g. OBS Main"
							class="input"
						/>
					</div>
				</div>
				{#if protocol === 'srt'}
					<div>
						<label class="field-label" for="source-passphrase">Passphrase (optional, min 10 chars)</label>
						<input
							id="source-passphrase"
							bind:value={srtPassphrase}
							placeholder="Passphrase"
							class="input"
						/>
					</div>
				{/if}
				<button type="submit" disabled={creating || !name.trim()} class="btn">
					{creating ? 'Adding…' : '+ Add Source'}
				</button>
			</form>
		</div>
	</section>

	<section class="panel">
		<header class="panel__head">▮ Sources</header>
		<div class="panel__body">
			{#if $sources.length === 0}
				<p class="text-xs text-amber-muted">No sources yet. Create one above.</p>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2">
					{#each $sources as source (source.id)}
						<SourceCard {source} ondelete={refresh} />
					{/each}
				</div>
			{/if}
		</div>
	</section>
</div>
