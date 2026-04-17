<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
  let { title = 'Instance Offline', message = 'The instance is not currently running. Start it from the dashboard to continue.' } = $props();
</script>

<div class="offline-state">
  <div class="offline-icon">⊘</div>
  <h3 class="offline-title">{title}</h3>
  <p class="offline-message">{message}</p>
  <button class="btn btn-primary">Start Instance</button>
</div>

<style>
  .offline-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 24px;
    text-align: center;
    color: rgba(255, 255, 255, 0.5);
  }

  .offline-icon {
    font-size: 48px;
    margin-bottom: 16px;
    opacity: 0.4;
  }

  .offline-title {
    font-size: 18px;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.7);
    margin-bottom: 8px;
  }

  .offline-message {
    font-size: 13px;
    margin-bottom: 24px;
    max-width: 300px;
    line-height: 1.6;
  }
</style>
