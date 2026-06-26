<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import '../../app.css';
	let { children } = $props();
</script>

<svelte:head>
	<title>Muxshed</title>
</svelte:head>

<div class="min-h-screen bg-bg p-3 font-mono text-amber-dim">
	{@render children()}
</div>
