<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import '../../app.css';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { connectWs, disconnectWs } from '$lib/ws';
	import { hasSession, setSessionToken, clearSession, setApiKey, api } from '$lib/api';
	import { pipelineState } from '$lib/stores/pipeline';

	let { children } = $props();

	type AppMode = 'loading' | 'setup' | 'setup-done' | 'login' | 'ready';
	let mode = $state<AppMode>('loading');

	// Setup state
	let setupName = $state('');
	let setupError = $state('');
	let setupLoading = $state(false);
	let setupResult = $state<{ username: string; api_key: string; message: string } | null>(null);
	let keyCopied = $state(false);

	// Login state
	let loginUsername = $state('');
	let loginPassword = $state('');
	let loginError = $state('');
	let loginLoading = $state(false);

	onMount(() => {
		checkSetup();
		return () => disconnectWs();
	});

	async function checkSetup() {
		try {
			const res = await fetch('/api/v1/setup/status');
			const data = await res.json();

			if (data.needs_setup) {
				mode = 'setup';
			} else if (hasSession()) {
				try {
					await api.me();
					mode = 'ready';
					connectWs();
				} catch {
					clearSession();
					mode = 'login';
				}
			} else {
				mode = 'login';
			}
		} catch {
			mode = hasSession() ? 'ready' : 'login';
			if (hasSession()) connectWs();
		}
	}

	async function runSetup() {
		setupLoading = true;
		setupError = '';
		try {
			const res = await fetch('/api/v1/setup/init', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ instance_name: setupName.trim() || null }),
			});
			if (!res.ok) {
				const body = await res.json().catch(() => ({ error: { message: 'Setup failed' } }));
				throw new Error(body.error?.message || 'Setup failed');
			}
			setupResult = await res.json();
			mode = 'setup-done';
		} catch (e) {
			setupError = e instanceof Error ? e.message : 'Setup failed';
		} finally {
			setupLoading = false;
		}
	}

	async function copyKey() {
		if (setupResult) {
			await navigator.clipboard.writeText(setupResult.api_key);
			keyCopied = true;
			setTimeout(() => (keyCopied = false), 2000);
		}
	}

	function proceedToLogin() {
		if (setupResult) {
			setApiKey(setupResult.api_key);
		}
		loginUsername = 'admin';
		loginPassword = '';
		mode = 'login';
	}

	async function doLogin() {
		loginLoading = true;
		loginError = '';
		try {
			const result = await api.login(loginUsername, loginPassword);
			setSessionToken(result.token);
			mode = 'ready';
			connectWs();
		} catch (e) {
			loginError = e instanceof Error ? e.message : 'Login failed';
		} finally {
			loginLoading = false;
		}
	}

	async function doLogout() {
		try {
			await api.logout();
		} catch {
			// ignore logout errors
		}
		clearSession();
		disconnectWs();
		mode = 'login';
		loginUsername = '';
		loginPassword = '';
		loginError = '';
	}

	const nav = [
		{ href: '/', label: 'Studio' },
		{ href: '/sources', label: 'Sources' },
		{ href: '/library', label: 'Library' },
		{ href: '/destinations', label: 'Destinations' },
		{ href: '/overlays', label: 'Overlays' },
		{ href: '/settings', label: 'Settings' },
	];

	function stateLabel(state: typeof $pipelineState): string {
		return state.state.toUpperCase();
	}

	function stateColor(state: typeof $pipelineState): string {
		switch (state.state) {
			case 'live': return 'bg-red-500';
			case 'starting':
			case 'stopping': return 'bg-yellow-500';
			case 'error': return 'bg-red-700';
			default: return 'bg-neutral-600';
		}
	}
</script>

<svelte:head>
	<title>Muxshed</title>
</svelte:head>

{#if mode === 'loading'}
	<div class="flex min-h-screen items-center justify-center">
		<span class="text-sm text-neutral-500">Loading...</span>
	</div>

{:else if mode === 'setup'}
	<div class="flex min-h-screen items-center justify-center">
		<div class="w-full max-w-md rounded-lg border border-neutral-700 bg-neutral-900 p-8">
			<h1 class="mb-2 text-2xl font-bold">Welcome to Muxshed</h1>
			<p class="mb-6 text-sm text-neutral-400">
				Your self-hosted live production studio. Let's get set up.
			</p>
			<p class="mb-4 text-sm text-neutral-400">
				This will create an admin account with default credentials. You can change your password after logging in.
			</p>
			<form onsubmit={(e) => { e.preventDefault(); runSetup(); }}>
				<label class="mb-1 block text-xs font-medium text-neutral-400">Instance name (optional)</label>
				<input
					bind:value={setupName}
					placeholder="e.g. My Studio"
					class="mb-4 w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				/>
				<button
					type="submit"
					disabled={setupLoading}
					class="w-full rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
				>
					{setupLoading ? 'Setting up...' : 'Set Up Muxshed'}
				</button>
			</form>
			{#if setupError}
				<p class="mt-3 text-sm text-red-400">{setupError}</p>
			{/if}
		</div>
	</div>

{:else if mode === 'setup-done' && setupResult}
	<div class="flex min-h-screen items-center justify-center">
		<div class="w-full max-w-md rounded-lg border border-neutral-700 bg-neutral-900 p-8">
			<h1 class="mb-2 text-2xl font-bold">Setup Complete</h1>

			<div class="mb-4 rounded border border-neutral-700 bg-neutral-800 p-4">
				<h2 class="mb-2 text-sm font-semibold text-neutral-300">Admin Login</h2>
				<div class="grid grid-cols-2 gap-2 text-sm">
					<span class="text-neutral-400">Username:</span>
					<span class="font-mono text-white">admin</span>
					<span class="text-neutral-400">Password:</span>
					<span class="font-mono text-white">admin</span>
				</div>
				<p class="mt-2 text-xs text-yellow-400">Change this password after first login.</p>
			</div>

			<div class="mb-4 rounded border border-neutral-700 bg-neutral-800 p-4">
				<h2 class="mb-2 text-sm font-semibold text-neutral-300">API Key (for Stream Deck / external tools)</h2>
				<code class="block break-all text-xs text-white">{setupResult.api_key}</code>
				<div class="mt-2">
					<button
						onclick={copyKey}
						class="rounded bg-neutral-700 px-3 py-1 text-xs text-white hover:bg-neutral-600"
					>
						{keyCopied ? 'Copied' : 'Copy Key'}
					</button>
				</div>
				<p class="mt-2 text-xs text-neutral-500">Save this key. It will not be shown again.</p>
			</div>

			<button
				onclick={proceedToLogin}
				class="w-full rounded bg-green-600 px-4 py-2 text-sm font-medium text-white hover:bg-green-700"
			>
				Continue to Login
			</button>
		</div>
	</div>

{:else if mode === 'login'}
	<div class="flex min-h-screen items-center justify-center">
		<div class="w-full max-w-sm rounded-lg border border-neutral-700 bg-neutral-900 p-8">
			<h1 class="mb-6 text-2xl font-bold">Muxshed</h1>
			<form onsubmit={(e) => { e.preventDefault(); doLogin(); }}>
				<label class="mb-1 block text-xs font-medium text-neutral-400">Username</label>
				<input
					bind:value={loginUsername}
					type="text"
					autocomplete="username"
					class="mb-3 w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				/>
				<label class="mb-1 block text-xs font-medium text-neutral-400">Password</label>
				<input
					bind:value={loginPassword}
					type="password"
					autocomplete="current-password"
					class="mb-4 w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				/>
				<button
					type="submit"
					disabled={loginLoading || !loginUsername || !loginPassword}
					class="w-full rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
				>
					{loginLoading ? 'Signing in...' : 'Sign In'}
				</button>
			</form>
			{#if loginError}
				<p class="mt-3 text-sm text-red-400">{loginError}</p>
			{/if}
		</div>
	</div>

{:else}
	<div class="flex min-h-screen">
		<nav class="flex w-48 flex-col border-r border-neutral-800 bg-neutral-900 p-4">
			<div class="mb-6 text-lg" style="font-family: var(--font-heading); letter-spacing: -0.02em;">Muxshed</div>
			{#each nav as item}
				<a
					href={item.href}
					class="rounded px-3 py-2 text-sm transition-colors {$page.url.pathname === item.href
						? 'bg-neutral-800 text-white'
						: 'text-neutral-400 hover:bg-neutral-800 hover:text-white'}"
				>
					{item.label}
				</a>
			{/each}
			<div class="mt-auto space-y-2 pt-4">
				<div class="flex items-center gap-2 text-xs">
					<span class="h-2 w-2 rounded-full {stateColor($pipelineState)}"></span>
					<span class="text-neutral-400">{stateLabel($pipelineState)}</span>
				</div>
				<button
					onclick={doLogout}
					class="w-full rounded px-3 py-1.5 text-left text-xs text-neutral-500 hover:bg-neutral-800 hover:text-neutral-300"
				>
					Sign Out
				</button>
			</div>
		</nav>
		<main class="flex-1 p-6">
			{@render children()}
		</main>
	</div>
{/if}
