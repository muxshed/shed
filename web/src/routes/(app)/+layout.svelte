<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import '../../app.css';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { connectWs, disconnectWs } from '$lib/ws';
	import { hasSession, setSessionToken, clearSession, setApiKey, api } from '$lib/api';
	import { pipelineState } from '$lib/stores/pipeline';
	import { notify } from '$lib/notify';

	let { children } = $props();

	type AppMode = 'loading' | 'setup' | 'setup-done' | 'login' | 'ready';
	let mode = $state<AppMode>('loading');

	// Setup state
	let setupName = $state('');
	let setupLoading = $state(false);
	let setupResult = $state<{ username: string; api_key: string; message: string } | null>(null);
	let keyCopied = $state(false);

	// Login state
	let loginUsername = $state('');
	let loginPassword = $state('');
	let loginLoading = $state(false);

	// Effects toggle + live clock
	let fxOff = $state(false);
	let now = $state(Date.now());

	onMount(() => {
		fxOff = localStorage.getItem('muxshed-fx') === 'off';
		applyFx();
		checkSetup();
		const t = setInterval(() => (now = Date.now()), 1000);
		return () => {
			clearInterval(t);
			disconnectWs();
		};
	});

	function applyFx() {
		document.documentElement.dataset.fx = fxOff ? 'off' : '';
	}

	function toggleFx() {
		fxOff = !fxOff;
		localStorage.setItem('muxshed-fx', fxOff ? 'off' : 'on');
		applyFx();
	}

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
			notify.error(e);
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
		try {
			const result = await api.login(loginUsername, loginPassword);
			setSessionToken(result.token);
			mode = 'ready';
			connectWs();
		} catch (e) {
			notify.error(e);
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
	}

	const nav = [
		{ href: '/', label: 'Studio' },
		{ href: '/sources', label: 'Sources' },
		{ href: '/library', label: 'Library' },
		{ href: '/destinations', label: 'Destinations' },
		{ href: '/guests', label: 'Guests' },
		{ href: '/channel', label: 'Channel' },
		{ href: '/settings', label: 'Settings' },
	];

	const crumb = $derived(
		nav.find((n) => n.href === $page.url.pathname)?.label ??
			($page.url.pathname.slice(1).split('/')[0] || 'Studio'),
	);

	function stateLabel(state: typeof $pipelineState): string {
		return state.state.toUpperCase();
	}

	// state -> pill modifier
	function statePill(state: typeof $pipelineState): string {
		switch (state.state) {
			case 'live':
				return 'pill--live';
			case 'starting':
			case 'stopping':
				return 'pill--warning';
			case 'error':
				return 'pill--rec';
			default:
				return 'pill--idle';
		}
	}

	const liveStartedAt = $derived(
		($pipelineState as { started_at?: string }).started_at ?? null,
	);

	const elapsed = $derived.by(() => {
		if ($pipelineState.state !== 'live' || !liveStartedAt) return '00:00:00';
		const secs = Math.max(0, Math.floor((now - new Date(liveStartedAt).getTime()) / 1000));
		const h = String(Math.floor(secs / 3600)).padStart(2, '0');
		const m = String(Math.floor((secs % 3600) / 60)).padStart(2, '0');
		const s = String(secs % 60).padStart(2, '0');
		return `${h}:${m}:${s}`;
	});
</script>

<svelte:head>
	<title>Muxshed</title>
</svelte:head>

{#if mode === 'loading'}
	<div class="flex min-h-screen items-center justify-center">
		<span class="label animate-pulse">Booting…</span>
	</div>

{:else if mode === 'setup'}
	<div class="flex min-h-screen items-center justify-center p-6">
		<div class="panel w-full max-w-md">
			<div class="panel__head">▮ First-time setup</div>
			<div class="panel__body">
				<h1 class="mb-2 text-lg text-amber-bright tracking-widest">WELCOME TO MUXSHED</h1>
				<p class="mb-4 text-xs text-amber-dim leading-relaxed">
					Your self-hosted live production studio. This creates an admin account with default
					credentials — you can change the password after logging in.
				</p>
				<form onsubmit={(e) => { e.preventDefault(); runSetup(); }}>
					<label class="field-label" for="setup-name">Instance name (optional)</label>
					<input id="setup-name" bind:value={setupName} placeholder="e.g. My Studio" class="input mb-4" />
					<button type="submit" disabled={setupLoading} class="btn w-full">
						{setupLoading ? 'Setting up…' : 'Set up Muxshed'}
					</button>
				</form>
			</div>
		</div>
	</div>

{:else if mode === 'setup-done' && setupResult}
	<div class="flex min-h-screen items-center justify-center p-6">
		<div class="panel w-full max-w-md">
			<div class="panel__head">▮ Setup complete</div>
			<div class="panel__body">
				<div class="row mb-3 flex-col items-stretch gap-2">
					<span class="label">Admin login</span>
					<div class="grid grid-cols-2 gap-1 text-xs">
						<span class="text-amber-dim">Username</span><span class="text-amber-bright">admin</span>
						<span class="text-amber-dim">Password</span><span class="text-amber-bright">admin</span>
					</div>
					<p class="text-[11px] text-warning">Change this password after first login.</p>
				</div>

				<div class="row mb-3 flex-col items-stretch gap-2">
					<span class="label">API key (Stream Deck / external tools)</span>
					<code class="block break-all text-xs text-amber-bright">{setupResult.api_key}</code>
					<button onclick={copyKey} class="btn btn--ghost self-start">
						{keyCopied ? '✓ Copied' : 'Copy key'}
					</button>
					<p class="text-[11px] text-amber-muted">Save this key. It will not be shown again.</p>
				</div>

				<button onclick={proceedToLogin} class="btn btn--go w-full">Continue to login</button>
			</div>
		</div>
	</div>

{:else if mode === 'login'}
	<div class="flex min-h-screen items-center justify-center p-6">
		<div class="panel w-full max-w-sm">
			<div class="panel__head">◉ Muxshed</div>
			<div class="panel__body">
				<form onsubmit={(e) => { e.preventDefault(); doLogin(); }}>
					<label class="field-label" for="login-user">Username</label>
					<input id="login-user" bind:value={loginUsername} type="text" autocomplete="username" class="input mb-3" />
					<label class="field-label" for="login-pass">Password</label>
					<input id="login-pass" bind:value={loginPassword} type="password" autocomplete="current-password" class="input mb-4" />
					<button type="submit" disabled={loginLoading || !loginUsername || !loginPassword} class="btn w-full">
						{loginLoading ? 'Signing in…' : 'Sign in'}
					</button>
				</form>
			</div>
		</div>
	</div>

{:else}
	<div class="flex min-h-screen flex-col">
		<!-- Status bar -->
		<header class="flex items-center justify-between border-b border-border bg-panel-raised px-3" style="height:40px">
			<div class="flex items-center gap-2 text-[13px]">
				<span class="text-amber-bright tracking-widest">◉ MUXSHED</span>
				<span class="text-amber-muted">/</span>
				<span class="label">{crumb}</span>
			</div>
			<div class="flex items-center gap-3">
				<span class="pill {statePill($pipelineState)}">
					{$pipelineState.state === 'live' ? '● ON AIR' : `○ ${stateLabel($pipelineState)}`}
				</span>
				{#if $pipelineState.state === 'live'}
					<span class="led text-[18px]">{elapsed}</span>
				{/if}
				<button class="btn btn--icon" onclick={toggleFx} title="Toggle screen effects" aria-label="Toggle screen effects">
					{fxOff ? '◍' : '◉'}
				</button>
			</div>
		</header>

		<div class="flex flex-1 min-h-0">
			<nav class="flex w-[180px] flex-col border-r border-border bg-panel py-2">
				{#each nav as item}
					<a href={item.href} class="rack-item {$page.url.pathname === item.href ? 'rack-item--active' : ''}">
						<span class="text-amber-muted">▸</span>{item.label}
					</a>
				{/each}
				<div class="mt-auto flex flex-col gap-1 px-3 pt-4">
					<div class="flex items-center gap-2 text-[11px] text-amber-dim">
						<span class="h-2 w-2 rounded-full" style="background: {$pipelineState.state === 'live' ? 'var(--color-live-glow)' : $pipelineState.state === 'error' ? 'var(--color-danger)' : 'var(--color-amber-muted)'}"></span>
						<span>{stateLabel($pipelineState)}</span>
					</div>
					<button onclick={doLogout} class="btn btn--ghost justify-start px-0">Sign out</button>
				</div>
			</nav>
			<main class="flex-1 overflow-y-auto p-6">
				{@render children()}
			</main>
		</div>
	</div>
{/if}
