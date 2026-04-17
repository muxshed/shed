<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import type { ApiKey } from '$lib/types';

	let keys = $state<ApiKey[]>([]);
	let name = $state('');
	let creating = $state(false);
	let newKey = $state('');
	let error = $state('');

	onMount(refresh);

	async function refresh() {
		keys = await api.listKeys();
	}

	async function create() {
		if (!name.trim()) return;
		creating = true;
		error = '';
		newKey = '';
		try {
			const result = await api.createKey(name.trim(), ['read', 'control', 'admin']);
			newKey = result.key;
			name = '';
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create key';
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

<div class="mx-auto max-w-2xl">
	<h1 class="mb-6 text-2xl font-bold">API Keys</h1>

	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Create New Key</h2>
		<form onsubmit={(e) => { e.preventDefault(); create(); }} class="flex gap-3">
			<input
				bind:value={name}
				placeholder="Key name"
				class="flex-1 rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<button
				type="submit"
				disabled={creating || !name.trim()}
				class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
			>
				Create
			</button>
		</form>
		{#if error}
			<p class="mt-2 text-sm text-red-400">{error}</p>
		{/if}
	</div>

	{#if newKey}
		<div class="mb-6 rounded-lg border border-yellow-800 bg-yellow-950 p-4">
			<p class="mb-2 text-sm text-yellow-300">
				Save this key now. It will not be shown again.
			</p>
			<div class="flex items-center gap-2">
				<code class="flex-1 break-all rounded bg-neutral-800 p-2 text-xs text-white">{newKey}</code>
				<button
					onclick={copyKey}
					class="rounded bg-neutral-700 px-3 py-1 text-xs text-white hover:bg-neutral-600"
				>
					Copy
				</button>
			</div>
		</div>
	{/if}

	{#if keys.length === 0}
		<p class="text-sm text-neutral-500">No API keys.</p>
	{:else}
		<div class="space-y-2">
			{#each keys as key (key.id)}
				<div class="flex items-center justify-between rounded-lg border border-neutral-700 bg-neutral-900 p-3">
					<div>
						<span class="text-sm font-medium">{key.name}</span>
						<span class="ml-2 text-xs text-neutral-500">
							Created {new Date(key.created_at).toLocaleDateString()}
						</span>
					</div>
					<button
						onclick={() => remove(key.id)}
						class="rounded bg-neutral-700 px-3 py-1 text-xs text-red-400 hover:bg-neutral-600"
					>
						Delete
					</button>
				</div>
			{/each}
		</div>
	{/if}
</div>
