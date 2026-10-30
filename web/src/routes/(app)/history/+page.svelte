<script lang="ts">
    import { onMount } from 'svelte';
    import { api, downloadHistoryCsv, type HistoryReport, type HistoryEvent } from '$lib/api';
    import { notify } from '$lib/notify';

    let days = $state(1);
    let report = $state<HistoryReport | null>(null);
    let loading = $state(true);
    let saving = $state(false);
    let retention = $state(90);
    let lastRefreshed = $state('');

    onMount(() => { void reload(); });

    async function reload() {
        loading = true;
        try {
            const result = await api.getHistory(days);
            report = result;
            retention = result.retention_days;
            lastRefreshed = new Date().toLocaleTimeString();
        } catch (e) { notify.error(e); }
        finally { loading = false; }
    }

    async function changeDays(next: number) {
        days = next;
        await reload();
    }

    async function saveRetention() {
        saving = true;
        try {
            await api.saveHistorySettings(Number(retention));
            notify.success('History retention saved');
            await reload();
        } catch (e) { notify.error(e); }
        finally { saving = false; }
    }

    async function exportFile(which: 'events' | 'sessions') {
        try { await downloadHistoryCsv(which, days); }
        catch (e) { notify.error(e); }
    }

    function time(value: string | null | undefined) {
        if (!value) return '—';
        const date = new Date(value);
        return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
    }
    function duration(seconds: number) {
        if (seconds <= 0) return '0m';
        const h = Math.floor(seconds / 3600);
        const m = Math.floor((seconds % 3600) / 60);
        return h ? `${h}h ${m}m` : `${m}m`;
    }
    function eventLabel(kind: string) {
        return kind.replaceAll('_', ' ');
    }
    function eventTone(event: HistoryEvent) {
        return ['camera_offline', 'encoder_stalled', 'failover_started'].includes(event.event_type)
            ? 'text-warning' : 'text-amber-bright';
    }
</script>

<svelte:head><title>Stream History & Reports | Muxshed</title></svelte:head>
<div class="mx-auto max-w-[1300px] space-y-3">
    <div class="flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div>
            <h1 class="text-xl font-semibold text-amber-bright">Stream History & Reliability</h1>
            <p class="text-xs text-amber-muted">Locally observed broadcast sessions, camera incidents, commercials, and encoder activity.</p>
        </div>
        <div class="flex flex-wrap gap-2 items-center">
            {#each [1, 7, 30] as selection}
                <button type="button" class={days === selection ? 'btn btn--go' : 'btn'} onclick={() => changeDays(selection)}>{selection === 1 ? '24 hours' : `${selection} days`}</button>
            {/each}
            <button type="button" class="btn" onclick={reload} disabled={loading}>Refresh</button>
        </div>
    </div>
    {#if loading && !report}
        <p class="text-amber-muted">Loading saved stream history…</p>
    {:else if report}
        <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <section class="panel"><div class="panel__body"><div class="text-xs text-amber-muted">Broadcast sessions</div><div class="text-2xl font-semibold text-amber-bright">{report.summary.sessions}</div><div class="text-xs text-amber-muted">Observed in selected period</div></div></section>
            <section class="panel"><div class="panel__body"><div class="text-xs text-amber-muted">Observed broadcast time</div><div class="text-2xl font-semibold text-amber-bright">{duration(report.summary.observed_broadcast_seconds)}</div><div class="text-xs text-amber-muted">Session heartbeats, not a YouTube SLA</div></div></section>
            <section class="panel"><div class="panel__body"><div class="text-xs text-amber-muted">Camera outages / recoveries</div><div class="text-2xl font-semibold text-amber-bright">{report.summary.camera_outages} / {report.summary.recoveries}</div><div class="text-xs text-amber-muted">Detected local source transitions</div></div></section>
            <section class="panel"><div class="panel__body"><div class="text-xs text-amber-muted">Local encoder health</div><div class="text-2xl font-semibold text-amber-bright">{report.summary.estimated_local_encoder_health_percent == null ? 'N/A' : `${report.summary.estimated_local_encoder_health_percent}%`}</div><div class="text-xs text-amber-muted">{report.summary.encoder_observed_samples} samples; not verified by YouTube</div></div></section>
        </div>
        <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
            <section class="panel"><div class="panel__body text-sm"><span class="text-amber-muted">Commercial starts</span><div class="text-lg font-semibold">{report.summary.ad_starts}</div></div></section>
            <section class="panel"><div class="panel__body text-sm"><span class="text-amber-muted">Commercial ends</span><div class="text-lg font-semibold">{report.summary.ad_ends}</div></div></section>
            <section class="panel"><div class="panel__body text-sm"><span class="text-amber-muted">Standby activations</span><div class="text-lg font-semibold">{report.summary.failover_events}</div></div></section>
            <section class="panel"><div class="panel__body text-sm"><span class="text-amber-muted">Encoder stalls</span><div class="text-lg font-semibold">{report.summary.encoder_alerts}</div></div></section>
        </div>
        <section class="panel">
            <header class="panel__head flex items-center justify-between"><span>Local encoder samples by day</span><span class="text-xs text-amber-muted">{lastRefreshed ? `Refreshed ${lastRefreshed}` : ''}</span></header>
            <div class="panel__body space-y-3">
                {#if report.daily.length === 0}
                    <p class="text-sm text-amber-muted">No measurements yet. The first sample is collected after the history monitor starts, then every 30 seconds.</p>
                {:else}
                    {#each report.daily as sample (sample.day)}
                        {@const percent = sample.encoder_observed_samples > 0 ? Math.round(sample.healthy_encoder_samples / sample.encoder_observed_samples * 100) : null}
                        <div class="grid grid-cols-[92px_1fr_80px] gap-3 items-center text-xs">
                            <span class="text-amber-muted">{sample.day}</span>
                            <div class="h-3 bg-well rounded overflow-hidden" role="img" aria-label={`Local encoder healthy samples ${percent ?? 'not available'} percent`}>
                                <div class="h-full bg-[#3cbe80]" style={`width:${percent ?? 0}%`}></div>
                            </div>
                            <span class="text-right">{percent === null ? 'N/A' : `${percent}%`} <span class="text-amber-muted">({sample.encoder_observed_samples})</span></span>
                        </div>
                    {/each}
                {/if}
            </div>
        </section>
        <div class="grid gap-3 xl:grid-cols-2">
            <section class="panel">
                <header class="panel__head flex items-center justify-between"><span>Recent events</span><button class="btn" type="button" onclick={() => exportFile('events')}>Export events CSV</button></header>
                <div class="panel__body max-h-[440px] overflow-y-auto space-y-2">
                    {#if report.events.length === 0}
                        <p class="text-sm text-amber-muted">No recorded incidents yet. New events will appear when transitions occur.</p>
                    {:else}
                        {#each report.events as event (event.id)}
                            <div class="p-2 rounded border border-border-dim bg-well text-xs">
                                <div class="flex flex-wrap justify-between gap-2"><span class={`font-medium capitalize ${eventTone(event)}`}>{eventLabel(event.event_type)}</span><span class="text-amber-muted">{time(event.at_utc)}</span></div>
                                {#if event.subject}<div class="mt-1">{event.subject}</div>{/if}
                                {#if event.detail}<div class="mt-1 text-amber-muted">{event.detail}</div>{/if}
                                {#if event.duration_ms != null}<div class="mt-1 text-amber-muted">Observed duration: {Math.round(event.duration_ms/1000)}s</div>{/if}
                            </div>
                        {/each}
                    {/if}
                </div>
            </section>
            <section class="panel">
                <header class="panel__head flex items-center justify-between"><span>Broadcast sessions</span><button class="btn" type="button" onclick={() => exportFile('sessions')}>Export sessions CSV</button></header>
                <div class="panel__body max-h-[440px] overflow-y-auto space-y-2">
                    {#if report.sessions.length === 0}
                        <p class="text-sm text-amber-muted">No sessions observed yet. Historical broadcasts before installing Phase 4 are unavailable.</p>
                    {:else}
                        {#each report.sessions as session (session.id)}
                            <div class="p-2 rounded border border-border-dim bg-well text-xs">
                                <div class="flex justify-between gap-2"><span class="font-medium">{time(session.started_at)}</span><span class={session.ended_at ? 'text-amber-muted' : 'text-[#3cbe80]'}>{session.ended_at ? (session.end_reason ?? 'Ended') : 'OBSERVED LIVE'}</span></div>
                                <div class="mt-1 text-amber-muted">Last observed: {time(session.last_seen_at)}</div>
                                {#if session.ended_at}<div class="text-amber-muted">Ended: {time(session.ended_at)}</div>{/if}
                            </div>
                        {/each}
                    {/if}
                </div>
            </section>
        </div>
        <section class="panel">
            <header class="panel__head">History retention and limitations</header>
            <div class="panel__body space-y-3">
                <div class="flex flex-wrap gap-3 items-center">
                    <label for="retention" class="text-sm">Keep records for</label>
                    <select id="retention" class="input max-w-48" bind:value={retention}>
                        <option value={30}>30 days</option>
                        <option value={90}>90 days</option>
                        <option value={180}>180 days</option>
                        <option value={365}>365 days</option>
                    </select>
                    <button class="btn btn--go" disabled={saving} onclick={saveRetention}>{saving ? 'Saving…' : 'Save retention'}</button>
                </div>
                <p class="text-xs text-amber-muted">{report.notice}</p>
                <p class="text-xs text-amber-muted">Export limits: newest 50,000 rows per CSV. Display: newest 500 events and 500 sessions. UTC timestamps are converted to your browser’s timezone.</p>
            </div>
        </section>
    {/if}
</div>
