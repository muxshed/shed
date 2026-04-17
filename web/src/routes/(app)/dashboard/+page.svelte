<!-- src/routes/(app)/dashboard/+page.svelte -->
<script lang="ts">
  import StatCard from '../../../components/StatCard.svelte'
</script>

<div class="dashboard-container">
  <!-- Topbar -->
  <div class="topbar">
    <div>
      <div class="page-title">Dashboard</div>
      <div class="page-sub">paul.muxshed.com</div>
    </div>
    <div class="topbar-actions">
      <button class="btn btn-ghost">Open Studio ↗</button>
      <button class="btn btn-live">● Live · 1h 24m</button>
    </div>
  </div>

  <!-- Content -->
  <div class="content">
    <!-- Stats row -->
    <div class="stats-row">
      <StatCard
        label="Stream Status"
        value="Live"
        subtitle="Running 1h 24m 13s"
        accent="green"
      />
      <StatCard
        label="Active Sources"
        value="3"
        subtitle="2 RTMP · 1 WebRTC"
      />
      <StatCard
        label="Destinations"
        value="4"
        subtitle="All connected"
      />
      <StatCard
        label="Recording"
        value="1.4 GB"
        subtitle="● Recording · 1h 24m"
        accent="purple"
      />
    </div>

    <!-- TODO: Main grid with control panels -->
    <div class="placeholder">More content coming...</div>
  </div>
</div>

<style>
  .dashboard-container {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .topbar {
    padding: 16px 24px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
    background: rgba(13, 12, 24, 0.8);
    backdrop-filter: blur(8px);
    position: sticky;
    top: 0;
    z-index: 10;
  }

  .page-title {
    font-size: 18px;
    font-weight: 700;
    color: #f0eeff;
    letter-spacing: -0.3px;
  }

  .page-sub {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.35);
    margin-top: 1px;
  }

  .topbar-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
    transition: all 0.15s;
  }

  .btn-ghost {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.6);
    border-color: rgba(255, 255, 255, 0.08);
  }

  .btn-ghost:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.85);
  }

  .btn-live {
    background: linear-gradient(135deg, #2d7a35, #3ca643);
    color: #fff;
    border-color: rgba(60, 166, 67, 0.4);
    box-shadow: 0 0 14px rgba(60, 166, 67, 0.25);
  }

  .content {
    padding: 24px;
    flex: 1;
    overflow-y: auto;
  }

  .stats-row {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 14px;
    margin-bottom: 20px;
  }

  .placeholder {
    padding: 20px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px dashed rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    color: rgba(255, 255, 255, 0.3);
    text-align: center;
  }
</style>
