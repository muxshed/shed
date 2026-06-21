<!-- src/routes/(app)/dashboard/+page.svelte -->
<script lang="ts">
</script>

<div class="mx-auto max-w-[1400px]">
  <!-- Topbar -->
  <div class="mb-4 flex items-center justify-between border-b border-border pb-3">
    <div>
      <div class="text-[13px] tracking-widest text-amber-bright">DASHBOARD</div>
      <div class="label mt-0.5">paul.muxshed.com</div>
    </div>
    <div class="flex items-center gap-3">
      <a href="/" class="btn btn--ghost">Open Studio ▸</a>
      <span class="pill pill--live">● LIVE</span>
      <span class="led text-[18px]">1:24:13</span>
    </div>
  </div>

  <!-- Stats row -->
  <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
    <section class="panel">
      <header class="panel__head">▮ STREAM STATUS</header>
      <div class="panel__body">
        <div class="pill pill--live">● LIVE</div>
        <div class="mt-2 label">Running</div>
        <div class="led mt-1 text-[24px]">1:24:13</div>
      </div>
    </section>

    <section class="panel">
      <header class="panel__head">▮ ACTIVE SOURCES</header>
      <div class="panel__body">
        <div class="led text-[28px]">3</div>
        <div class="mt-2 text-amber-dim">2 RTMP · 1 WebRTC</div>
      </div>
    </section>

    <section class="panel">
      <header class="panel__head">▮ DESTINATIONS</header>
      <div class="panel__body">
        <div class="led text-[28px]">4</div>
        <div class="mt-2 text-amber-dim">All connected</div>
      </div>
    </section>

    <section class="panel">
      <header class="panel__head">▮ RECORDING</header>
      <div class="panel__body">
        <div class="led text-[24px]">1.4 GB</div>
        <div class="mt-2 flex items-center gap-2">
          <span class="pill pill--rec">● REC</span>
          <span class="text-amber-dim">1:24:00</span>
        </div>
      </div>
    </section>
  </div>

  <!-- Placeholder -->
  <div class="mt-5 rounded-md border border-border-dim bg-panel p-6 text-center text-amber-muted">
    More content coming…
  </div>
</div>
