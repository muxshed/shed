<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import '../../app.css';
	let { children } = $props();
</script>

<svelte:head>
	<title>Muxshed</title>
</svelte:head>

<div class="min-h-screen bg-neutral-950 p-3">
	{@render children()}
</div>
