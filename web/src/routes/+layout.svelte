<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import '@fontsource/jetbrains-mono/400.css';
	import '@fontsource/jetbrains-mono/500.css';
	import '@fontsource/jetbrains-mono/700.css';
	import 'dseg/css/dseg.css';
	import { Toaster } from '$lib/components/ui/sonner';

	let { children } = $props();
</script>

{@render children()}
<Toaster />
