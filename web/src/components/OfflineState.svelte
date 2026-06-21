<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
  let { title = 'Instance Offline', message = 'The instance is not currently running. Start it from the dashboard to continue.' } = $props();
</script>

<div class="flex flex-col items-center justify-center px-6 py-16 text-center">
  <div class="mb-4 text-[40px] leading-none text-amber-muted">○</div>
  <h3 class="mb-2 text-[13px] tracking-[1px] uppercase text-amber-dim">{title}</h3>
  <p class="mb-6 max-w-[300px] text-xs leading-relaxed text-amber-muted">{message}</p>
  <button class="btn btn--go">▸ Start Instance</button>
</div>
