<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { notify } from '$lib/notify';
	import type { ApiKey } from '$lib/types';

	let keys = $state<ApiKey[]>([]);
	let name = $state('');
	let creating = $state(false);
	let newKey = $state('');

	onMount(refresh);

	async function refresh() {
		keys = await api.listKeys();
	}

	async function create() {
		if (!name.trim()) return;
		creating = true;
		newKey = '';
		try {
			const result = await api.createKey(name.trim(), ['read', 'control', 'admin']);
			newKey = result.key;
			name = '';
			await refresh();
		} catch (e) {
			notify.error(e);
		} finally {
			creating = false;
		}
	}

	async function remove(id: string) {
		await api.deleteKey(id);
		await refresh();
	}

	async function copyKey() {
		await navigator.clipboard.writeText(newKey);
	}
</script>

<div class="mx-auto max-w-2xl space-y-4">
	<section class="panel">
		<header class="panel__head">▮ Create New Key</header>
		<div class="panel__body">
			<form onsubmit={(e) => { e.preventDefault(); create(); }} class="flex items-end gap-3">
				<div class="flex-1">
					<label class="field-label" for="key-name">Key name</label>
					<input id="key-name" bind:value={name} placeholder="e.g. Stream Deck" class="input" />
				</div>
				<button type="submit" disabled={creating || !name.trim()} class="btn">
					{creating ? 'Creating…' : '+ Create'}
				</button>
			</form>
		</div>
	</section>

	{#if newKey}
		<section class="panel" style="border-color:#7A7016">
			<header class="panel__head text-warning">▮ New Key — save it now</header>
			<div class="panel__body space-y-2">
				<p class="text-warning text-xs">Save this key now. It will not be shown again.</p>
				<div class="flex items-center gap-2">
					<code class="flex-1 break-all rounded-sm border border-border bg-well p-2 text-xs text-amber-bright">{newKey}</code>
					<button onclick={copyKey} class="btn btn--icon" title="Copy API key" aria-label="Copy API key">⧉</button>
				</div>
			</div>
		</section>
	{/if}

	<section class="panel">
		<header class="panel__head">▮ API Keys</header>
		<div class="panel__body">
			{#if keys.length === 0}
				<p class="text-amber-muted text-xs">No API keys.</p>
			{:else}
				<div class="space-y-2">
					{#each keys as key (key.id)}
						<div class="row">
							<div>
								<span class="text-amber">{key.name}</span>
								<span class="ml-2 text-xs text-amber-dim">
									Created {new Date(key.created_at).toLocaleDateString()}
								</span>
							</div>
							<button onclick={() => remove(key.id)} class="btn btn--danger" title="Delete key" aria-label="Delete API key {key.name}">
								Delete
							</button>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	</section>
</div>
