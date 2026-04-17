<!-- src/components/StatCard.svelte -->
<script lang="ts">
  export let label: string
  export let value: string | number
  export let subtitle = ''
  export let accent: 'green' | 'purple' | 'default' = 'default'
</script>

<div class="stat-card" class:accent-{accent}>
  <div class="stat-label">{label}</div>
  <div class="stat-value">{value}</div>
  {#if subtitle}
    <div class="stat-sub">{subtitle}</div>
  {/if}
</div>

<style>
  .stat-card {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 10px;
    padding: 16px;
    transition: border-color 0.15s;
  }

  .stat-card:hover {
    border-color: rgba(255, 255, 255, 0.12);
  }

  .stat-card.accent-purple {
    background: rgba(60, 52, 89, 0.2);
    border-color: rgba(90, 78, 138, 0.3);
  }

  .stat-card.accent-green {
    background: rgba(60, 166, 67, 0.08);
    border-color: rgba(60, 166, 67, 0.2);
  }

  .stat-label {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.8px;
    margin-bottom: 8px;
  }

  .stat-value {
    font-size: 22px;
    font-weight: 700;
    color: #f0eeff;
    letter-spacing: -0.5px;
    line-height: 1;
  }

  .stat-sub {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    margin-top: 5px;
  }
</style>
