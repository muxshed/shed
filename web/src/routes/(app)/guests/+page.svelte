<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { notify } from '$lib/notify';
	import type { Guest } from '$lib/types';

	let guests = $state<Guest[]>([]);
	let name = $state('');
	let creating = $state(false);
	let copied = $state<string | null>(null);

	onMount(load);

	async function load() {
		try {
			guests = await api.listGuests();
		} catch (e) {
			notify.error(e);
		}
	}

	async function invite(e: Event) {
		e.preventDefault();
		if (!name.trim() || creating) return;
		creating = true;
		try {
			await api.inviteGuest(name.trim());
			name = '';
			notify.success('Guest invited');
			await load();
		} catch (e) {
			notify.error(e);
		} finally {
			creating = false;
		}
	}

	async function remove(id: string) {
		try {
			await api.deleteGuest(id);
			await load();
		} catch (e) {
			notify.error(e);
		}
	}

	function linkFor(token: string): string {
		return `${location.origin}/guest/${token}`;
	}

	async function copy(token: string) {
		await navigator.clipboard.writeText(linkFor(token));
		copied = token;
		setTimeout(() => (copied = null), 1500);
	}
</script>

<div class="mx-auto max-w-4xl space-y-4">
	<section class="panel">
		<div class="panel__head">▮ Invite a guest</div>
		<div class="panel__body">
			<form onsubmit={invite} class="flex flex-col gap-2 sm:flex-row sm:items-end">
				<div class="flex-1">
					<label class="field-label" for="guest-name">Guest name</label>
					<input id="guest-name" class="input" bind:value={name} placeholder="e.g. Jordan" />
				</div>
				<button type="submit" disabled={creating || !name.trim()} class="btn">+ Invite</button>
			</form>
			<p class="mt-3 text-[12px] text-amber-muted">
				Send the generated link to your guest. They open it, allow camera/mic, and join the studio.
			</p>
		</div>
	</section>

	<section class="panel">
		<div class="panel__head">▮ Guests</div>
		<div class="panel__body space-y-2">
			{#if guests.length === 0}
				<p class="text-sm text-amber-muted">No guests yet. Invite one above.</p>
			{:else}
				{#each guests as g (g.id)}
					<div class="row flex-col items-stretch gap-2 sm:flex-row sm:items-center">
						<div class="flex items-center gap-3">
							<span class="text-amber-bright">{g.name}</span>
							<span class="pill {g.status === 'connected' ? 'pill--live' : 'pill--idle'}"
								>{g.status === 'connected' ? '● live' : '○ ' + g.status}</span
							>
						</div>
						<input class="input flex-1 sm:mx-3" readonly value={linkFor(g.token)} />
						<div class="flex gap-2">
							<button class="btn btn--ghost" onclick={() => copy(g.token)}
								>{copied === g.token ? '✓ Copied' : 'Copy link'}</button
							>
							<button class="btn btn--danger" onclick={() => remove(g.id)}>Delete</button>
						</div>
					</div>
				{/each}
			{/if}
		</div>
	</section>
</div>
