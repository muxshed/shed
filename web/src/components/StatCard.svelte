<!-- src/components/StatCard.svelte -->
<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
  let {
    label,
    value,
    subtitle = '',
    accent = 'default'
  }: {
    label: string
    value: string | number
    subtitle?: string
    accent?: 'green' | 'purple' | 'default'
  } = $props()

  const borderClass = $derived(
    accent === 'green' ? 'border-live/40' : accent === 'purple' ? 'border-amber/40' : 'border-border-dim'
  )
  const valueClass = $derived(accent === 'green' ? 'text-live' : 'led')
</script>

<div class="rounded-md border bg-panel-raised p-4 {borderClass}">
  <div class="label mb-2">{label}</div>
  <div class="text-[22px] leading-none {valueClass}">{value}</div>
  {#if subtitle}
    <div class="mt-1.5 text-[11px] text-amber-muted">{subtitle}</div>
  {/if}
</div>
