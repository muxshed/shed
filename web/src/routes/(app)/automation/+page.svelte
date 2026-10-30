<script lang="ts">
 import { onMount } from 'svelte';
 import { api, type BroadcastAutomationConfig, type BroadcastAutomationStatus } from '$lib/api';
 import type { Source } from '$lib/types';
 import { notify } from '$lib/notify';

 let loading = $state(true);
 let saving = $state(false);
 let status = $state<BroadcastAutomationStatus | null>(null);
 let config = $state<BroadcastAutomationConfig>({
  weather_enabled:false, weather_city:'Kingston', weather_latitude:17.997, weather_longitude:-76.7936,
  sponsor_enabled:false, sponsor_text:'', broadcast_overlay_enabled:false,
  weather_popup_interval_minutes:10, weather_popup_duration_seconds:15,
  auto_switch_enabled:false, switch_interval_minutes:30, switch_source_ids:[], alerts_enabled:false,
 });
 let choices = $state<Source[]>([]);
 let refreshTimer: ReturnType<typeof setInterval> | null = null;
 async function refresh() {
   try {
     const result = await api.getAutomation();
     status = result;
   } catch (e) { notify.error(e); }
 }
 onMount(() => {
   let alive = true;
   Promise.all([api.getAutomation(), api.listSources()]).then(([data, sourceList]) => {
     if (!alive) return;
     status = data; config = structuredClone(data.config); choices = sourceList;
   }).catch((e) => notify.error(e)).finally(() => { if (alive) loading = false; });
   refreshTimer = setInterval(() => { if (alive) refresh(); }, 5000);
   return () => { alive = false; if (refreshTimer) clearInterval(refreshTimer); };
 });
 function toggleSource(id: string, on: boolean) {
   config.switch_source_ids = on
    ? [...config.switch_source_ids.filter((v) => v !== id), id]
    : config.switch_source_ids.filter((v) => v !== id);
 }
 async function save() {
   if (saving) return;
   saving = true;
   try {
     const result = await api.setAutomation(config);
     status = result;
     config = structuredClone(result.config);
     notify.success('Broadcast automation settings saved');
   } catch(e) { notify.error(e); }
   finally { saving = false; }
 }
</script>
<svelte:head><title>Weather & Automation | Muxshed</title></svelte:head>
<div class="mx-auto max-w-[1160px] space-y-3">
 <div class="flex flex-wrap items-center justify-between gap-2">
  <div><h1 class="text-xl font-semibold text-amber-bright">Weather & Automation</h1>
  <p class="text-xs text-amber-muted">Optional broadcasting tools. All automations default to off until you enable them.</p></div>
  <a href="/" class="btn">Back to Studio</a>
 </div>
 {#if loading}<p class="text-amber-dim">Loading broadcast settings...</p>
 {:else}
  <div class="grid gap-3 lg:grid-cols-2">
   <section class="panel">
    <header class="panel__head">Weather lower-third</header>
    <div class="panel__body space-y-3">
     <label class="flex items-center gap-2"><input type="checkbox" bind:checked={config.weather_enabled}/> Fetch real-time weather</label>
     <label class="field-label" for="weather-city">Location label</label>
     <input id="weather-city" class="input" maxlength="48" bind:value={config.weather_city}/>
     <div class="grid grid-cols-2 gap-2">
      <div><label class="field-label" for="weather-lat">Latitude</label><input id="weather-lat" type="number" step="0.0001" min="-90" max="90" class="input" bind:value={config.weather_latitude}/></div>
      <div><label class="field-label" for="weather-lon">Longitude</label><input id="weather-lon" type="number" step="0.0001" min="-180" max="180" class="input" bind:value={config.weather_longitude}/></div>
     </div>
     {#if status?.weather}
      <div class="rounded border border-border bg-well p-3 text-sm">
       <strong class="text-amber-bright">{status.config.weather_city}: {Math.round(status.weather.temperature_c)} °C</strong>
       <div class="text-xs text-amber-dim">Humidity {Math.round(status.weather.humidity_percent)}% · Wind {Math.round(status.weather.wind_kmh)} km/h · Rain {status.weather.rainfall_mm} mm</div>
       <div class="mt-1 text-xs text-amber-muted">Updated {new Date(status.weather.updated_at_utc).toLocaleString()}</div>
      </div>
     {:else}<p class="text-xs text-amber-muted">No current reading. New readings refresh about every 10 minutes when enabled, and disappear when stale.</p>{/if}
     <div class="grid grid-cols-2 gap-2">
      <div><label class="field-label" for="weather-pop-interval">Show every (minutes)</label>
       <input id="weather-pop-interval" class="input" type="number" min="1" max="120" step="1" bind:value={config.weather_popup_interval_minutes}/></div>
      <div><label class="field-label" for="weather-pop-duration">Visible for (seconds)</label>
       <input id="weather-pop-duration" class="input" type="number" min="5" max="60" step="1" bind:value={config.weather_popup_duration_seconds}/></div>
     </div>
     <div class="rounded border border-border-dim bg-panel-raised px-3 py-2 text-xs">
      <span class="font-semibold text-amber-bright">Weather position: bottom left</span>
      <span class="ml-2 text-amber-muted">{status?.weather_popup_active ? 'ON SCREEN NOW' : status?.weather_popup_next_seconds != null ? `Next in ${Math.ceil(status.weather_popup_next_seconds / 60)} min` : 'Waiting for active broadcast'}</span>
     </div>
     <p class="text-xs text-amber-muted">Updated conditions come from Open-Meteo. A 10-minute popup cycle begins when the broadcast goes LIVE. Weather hides during commercials and failover.</p>
    </div>
   </section>
   <section class="panel">
    <header class="panel__head">Sponsor lower-third</header>
    <div class="panel__body space-y-3">
     <label class="flex items-center gap-2"><input type="checkbox" bind:checked={config.sponsor_enabled}/> Show sponsor text</label>
     <label class="field-label" for="sponsor">Sponsor name</label>
     <input id="sponsor" class="input" maxlength="100" placeholder="Your sponsor name" bind:value={config.sponsor_text}/>
     <label class="flex items-center gap-2"><input type="checkbox" bind:checked={config.broadcast_overlay_enabled}/> Enable text lower-third on outgoing YouTube video</label>
     <div class="rounded border border-border bg-well p-3 space-y-3">
      <p class="text-xs uppercase text-amber-muted">Outgoing video layout preview</p>
      <div class="relative overflow-hidden rounded-md border border-border-dim bg-[#303942]" style="aspect-ratio:16/9;">
       <div class="absolute bottom-[14%] left-[2.5%] max-w-[62%] rounded-md bg-[#0d2a42]/95 px-3 py-2.5 text-white shadow-lg">
        <div class="text-sm font-bold">{config.weather_city}  {status?.weather ? `${Math.round(status.weather.temperature_c)}°C` : 'Weather'}</div>
        <div class="text-[10px]">{status?.weather ? `Humidity ${Math.round(status.weather.humidity_percent)}%  Wind ${Math.round(status.weather.wind_kmh)} km/h` : 'Conditions appear when refreshed'}</div>
        <div class="text-[10px]">{status?.weather ? `Rain ${status.weather.rainfall_mm} mm` : ''}</div>
       </div>
       <div class="absolute bottom-[3%] right-[2.5%] max-w-[43%] rounded-md bg-[#142236]/95 px-3 py-1.5 text-right text-white shadow-lg">
        <div class="text-[9px] uppercase tracking-wide text-[#8ed7ff]">Sponsored by</div>
        <div class="text-sm font-bold">{config.sponsor_text || 'Sponsor'}</div>
       </div>
      </div>
      <p class="text-xs text-amber-muted">The sponsor stays at the bottom right. The weather card appears bottom left for {config.weather_popup_duration_seconds} seconds every {config.weather_popup_interval_minutes} minutes. This is a layout simulation. Studio now includes an optional layout preview on Program, but actual rendered graphics must still be verified on YouTube.</p>
     </div>
     <p class="text-xs text-warning">Enable the two-layer filter off-air. The initial enablement takes effect at the next encoder start. Later weather popups and sponsor text changes require no restart. The outgoing stream may be cropped by YouTube's vertical preview.</p>
    </div>
   </section>
   <section class="panel">
    <header class="panel__head">Scheduled camera switching</header>
    <div class="panel__body space-y-3">
     <label class="flex items-center gap-2"><input type="checkbox" bind:checked={config.auto_switch_enabled}/> Enable automatic switching</label>
     <label class="field-label" for="switch-min">Minutes between switches</label>
     <input id="switch-min" class="input max-w-32" type="number" min="1" max="1440" bind:value={config.switch_interval_minutes}/>
     <div class="space-y-2 rounded border border-border-dim bg-well p-2">
      {#each choices as src (src.id)}
       <label class="flex items-center gap-2 text-sm">
        <input type="checkbox" checked={config.switch_source_ids.includes(src.id)} onchange={(e) => toggleSource(src.id, e.currentTarget.checked)}/>
        <span>{src.name}</span>
       </label>
      {/each}
     </div>
     <p class="text-xs text-amber-muted">Select at least two sources. Offline cameras are skipped. Ads and active failover take priority. Test off-air: camera cuts may reconnect the YouTube encoder.</p>
    </div>
   </section>
   <section class="panel">
    <header class="panel__head">Failure notifications</header>
    <div class="panel__body space-y-3">
     <label class="flex items-center gap-2"><input type="checkbox" bind:checked={config.alerts_enabled}/> Log camera loss and recovery</label>
     <p class="text-xs text-amber-muted">In-app monitoring only. Email, SMS, and independent YouTube health checks need a separate connector.</p>
     {#if (status?.events.length || 0) > 0}
      <div class="max-h-60 space-y-2 overflow-auto">
       {#each status?.events || [] as event}
        <div class="rounded border border-border-dim bg-well p-2 text-xs">
         <span class="font-medium text-amber-bright">{event.message}</span>
         <p class="text-amber-muted">{new Date(event.at_utc).toLocaleString()}</p>
        </div>
       {/each}
      </div>
     {:else}<p class="text-xs text-amber-muted">No alerts since the last Muxshed restart.</p>{/if}
     <button class="btn" type="button" onclick={refresh}>Refresh status</button>
    </div>
   </section>
  </div>
  <div class="sticky bottom-2 z-10 flex flex-wrap items-center justify-between gap-2 rounded border border-border bg-panel-raised p-3 shadow-lg">
   <span class="text-xs text-amber-muted">Saving settings does not restart the livestream.</span>
   <button type="button" class="btn btn--go" disabled={saving} onclick={save}>{saving ? 'Saving…' : 'Save Automation Settings'}</button>
  </div>
 {/if}
</div>
