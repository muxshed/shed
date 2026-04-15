<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
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
