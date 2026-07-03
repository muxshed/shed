<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { notify } from '$lib/notify';
	import { activeSchedule } from '$lib/stores/pipeline';
	import type { Schedule, Asset, Destination } from '$lib/types';

	let schedules = $state<Schedule[]>([]);
	let assets = $state<Asset[]>([]);
	let destinations = $state<Destination[]>([]);
	let tz = $state('UTC');

	let editing = $state<Schedule | null>(null);
	let name = $state('');
	let triggerKind = $state<'once' | 'cron'>('cron');
	let cron = $state('0 20 * * *');
	let onceAt = $state('');
	let vodId = $state('');
	let standbyId = $state('');
	let endBehavior = $state<'stop' | 'loop' | 'standby'>('stop');
	let destIds = $state<string[]>([]);

	onMount(refresh);
	async function refresh() {
		const [s, a, d, t] = await Promise.all([
			api.listSchedules(),
			api.listAssets(),
			api.listDestinations(),
			api.getTimezone(),
		]);
		schedules = s;
		assets = a.filter((x) => x.asset_type === 'video');
		destinations = d;
		tz = t.timezone;
	}

	function newSchedule() {
		editing = { id: '' } as Schedule;
		name = '';
		triggerKind = 'cron';
		cron = '0 20 * * *';
		onceAt = '';
		vodId = assets[0]?.id ?? '';
		standbyId = '';
		endBehavior = 'stop';
		destIds = [];
	}

	function payload() {
		const trigger = triggerKind === 'cron' ? { kind: 'cron', expr: cron } : { kind: 'once', at: onceAt };
		return {
			name,
			enabled: true,
			trigger,
			destination_ids: destIds,
			standby_asset_id: standbyId || null,
			end_behavior: endBehavior,
			until_at: null,
			items: [{ kind: 'vod', ref_id: vodId }],
		};
	}

	async function save() {
		if (!vodId) {
			notify.error('Pick a VOD');
			return;
		}
		try {
			if (editing?.id) await api.updateSchedule(editing.id, payload());
			else await api.createSchedule(payload());
			editing = null;
			await refresh();
		} catch (e) {
			notify.error(e);
		}
	}

	async function toggle(s: Schedule) {
		try {
			await api.updateSchedule(s.id, {
				name: s.name,
				enabled: !s.enabled,
				trigger: s.trigger,
				destination_ids: s.destination_ids,
				standby_asset_id: s.standby_asset_id,
				end_behavior: s.end_behavior,
				until_at: s.until_at,
				items: s.items.map((i) => ({ kind: i.kind, ref_id: i.ref_id })),
			});
			await refresh();
		} catch (e) {
			notify.error(e);
		}
	}
	async function del(s: Schedule) {
		await api.deleteSchedule(s.id);
		await refresh();
	}
	async function runNow(s: Schedule) {
		try {
			await api.runSchedule(s.id);
		} catch (e) {
			notify.error(e);
		}
	}

	function nextRun(s: Schedule) {
		return s.next_run_at ? new Date(s.next_run_at).toLocaleString() : '—';
	}
</script>

<div class="mx-auto max-w-4xl">
	<div class="row mb-4 items-center justify-between">
		<div>
			<h1 class="text-lg text-amber-bright tracking-widest">SCHEDULES</h1>
			<p class="text-xs text-amber-dim">Timezone: {tz} · set it in Settings</p>
		</div>
		<button class="btn btn--go" onclick={newSchedule}>+ New schedule</button>
	</div>

	{#if $activeSchedule.id}
		<div class="panel mb-3">
			<div class="panel__body text-live">
				● A scheduled broadcast is on air.
				<button class="btn btn--ghost ml-2" onclick={() => api.stopSchedule()}>Stop</button>
			</div>
		</div>
	{/if}

	{#each schedules as s (s.id)}
		<div class="panel mb-2">
			<div class="panel__body row items-center justify-between">
				<div>
					<span class="text-amber">{s.name}</span>
					<span class="ml-2 text-[11px] text-amber-muted">
						{s.trigger.kind === 'cron' ? `cron ${s.trigger.expr}` : `once ${s.trigger.at}`} · next {nextRun(s)} · {s.end_behavior}
					</span>
				</div>
				<div class="flex gap-1">
					<button class="btn btn--ghost" onclick={() => runNow(s)}>Air now</button>
					<button class="btn {s.enabled ? 'btn--go' : ''}" onclick={() => toggle(s)}>{s.enabled ? 'On' : 'Off'}</button>
					<button class="btn btn--danger" onclick={() => del(s)}>✕</button>
				</div>
			</div>
		</div>
	{/each}

	{#if editing}
		<div class="panel mt-4">
			<div class="panel__body">
				<label class="field-label" for="s-name">Name</label>
				<input id="s-name" bind:value={name} class="input mb-3" />

				<label class="field-label" for="s-vod">VOD</label>
				<select id="s-vod" bind:value={vodId} class="select mb-3 w-full">
					{#each assets as a}<option value={a.id}>{a.name}</option>{/each}
				</select>

				<label class="field-label" for="s-trigger">Trigger</label>
				<select id="s-trigger" bind:value={triggerKind} class="select mb-2 w-full">
					<option value="cron">Recurring (cron)</option>
					<option value="once">One-off (date/time)</option>
				</select>
				{#if triggerKind === 'cron'}
					<input bind:value={cron} placeholder="0 20 * * *" class="input mb-3" />
				{:else}
					<input type="datetime-local" bind:value={onceAt} class="input mb-3" />
				{/if}

				<label class="field-label" for="s-end">When it ends</label>
				<select id="s-end" bind:value={endBehavior} class="select mb-3 w-full">
					<option value="stop">Stop (go offline)</option>
					<option value="loop">Loop</option>
					<option value="standby">Hold on standby</option>
				</select>

				<label class="field-label" for="s-standby">Standby card (optional)</label>
				<select id="s-standby" bind:value={standbyId} class="select mb-3 w-full">
					<option value="">— none —</option>
					{#each assets as a}<option value={a.id}>{a.name}</option>{/each}
				</select>

				<span class="field-label">Destinations</span>
				<div class="mb-3 flex flex-col gap-1">
					{#each destinations as d}
						<label class="flex items-center gap-2 text-xs">
							<input
								type="checkbox"
								value={d.id}
								checked={destIds.includes(d.id)}
								onchange={(e) =>
									(destIds = e.currentTarget.checked ? [...destIds, d.id] : destIds.filter((x) => x !== d.id))}
							/>
							{d.name}
						</label>
					{/each}
				</div>

				<div class="flex gap-2">
					<button class="btn btn--go" onclick={save}>Save</button>
					<button class="btn btn--ghost" onclick={() => (editing = null)}>Cancel</button>
				</div>
			</div>
		</div>
	{/if}
</div>
