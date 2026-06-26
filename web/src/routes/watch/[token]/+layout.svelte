<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import '../../../app.css';
	let { children } = $props();
</script>

<svelte:head>
	<title>Watch — Muxshed</title>
</svelte:head>

<div class="min-h-screen bg-bg font-mono text-amber-dim">
	{@render children()}
</div>
