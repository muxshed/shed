<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	let currentKey = $state(
		typeof window !== 'undefined' ? localStorage.getItem('muxshed_api_key') || '' : '',
	);
	let saved = $state(false);

	function saveKey() {
		localStorage.setItem('muxshed_api_key', currentKey);
		saved = true;
		setTimeout(() => (saved = false), 2000);
	}

	function clearKey() {
		localStorage.removeItem('muxshed_api_key');
		currentKey = '';
		window.location.reload();
	}
</script>

<div class="mx-auto max-w-2xl">
	<h1 class="mb-6 text-2xl font-bold">Settings</h1>

	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">API Key</h2>
		<div class="flex gap-3">
			<input
				bind:value={currentKey}
				type="password"
				placeholder="mxs_..."
				class="flex-1 rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white font-mono focus:border-blue-500 focus:outline-none"
			/>
			<button
				onclick={saveKey}
				class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
			>
				{saved ? 'Saved' : 'Save'}
			</button>
			<button
				onclick={clearKey}
				class="rounded bg-neutral-700 px-4 py-2 text-sm text-neutral-300 hover:bg-neutral-600"
			>
				Clear
			</button>
		</div>
	</div>

	<a
		href="/settings/keys"
		class="block rounded-lg border border-neutral-700 bg-neutral-900 p-4 text-sm text-neutral-300 hover:bg-neutral-800"
	>
		Manage API Keys
	</a>
</div>
