<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { notify } from '$lib/notify';
	import type { User, Source, FailoverConfig } from '$lib/types';

	// Password change
	let currentPassword = $state('');
	let newPassword = $state('');
	let confirmPassword = $state('');
	let pwSuccess = $state(false);
	let pwLoading = $state(false);

	// Users
	let users = $state<User[]>([]);
	let newUsername = $state('');
	let newUserPassword = $state('');
	let newUserRole = $state<'admin' | 'write' | 'read'>('read');
	let userLoading = $state(false);
	let editingUser = $state<string | null>(null);
	let editRole = $state('');

	// Screen effects
	let fxOff = $state(false);

	// Program failover
	let failover = $state<FailoverConfig>({
		enabled: false,
		fallback_source_id: null,
		offline_grace_ms: 3000,
		recovery_grace_ms: 3000,
	});
	let sources = $state<Source[]>([]);
	let failoverSaving = $state(false);

	onMount(async () => {
		fxOff = localStorage.getItem('muxshed-fx') === 'off';
		await refreshUsers();
		try {
			[failover, sources] = await Promise.all([api.getFailover(), api.listSources()]);
		} catch (e) {
			notify.error(e);
		}
	});

	async function saveFailover() {
		failoverSaving = true;
		try {
			failover = await api.setFailover({
				...failover,
				offline_grace_ms: Math.max(500, Math.round(failover.offline_grace_ms)),
				recovery_grace_ms: Math.max(500, Math.round(failover.recovery_grace_ms)),
			});
			notify.success('Failover settings saved');
		} catch (e) {
			notify.error(e);
		} finally {
			failoverSaving = false;
		}
	}

	function toggleFx() {
		fxOff = !fxOff;
		localStorage.setItem('muxshed-fx', fxOff ? 'off' : 'on');
		document.documentElement.dataset.fx = fxOff ? 'off' : '';
	}

	async function refreshUsers() {
		users = await api.listUsers();
	}

	async function changePassword() {
		pwSuccess = false;
		if (newPassword.length < 6) {
			notify.error('Password must be at least 6 characters');
			return;
		}
		if (newPassword !== confirmPassword) {
			notify.error('Passwords do not match');
			return;
		}
		pwLoading = true;
		try {
			await api.changePassword(currentPassword, newPassword);
			pwSuccess = true;
			currentPassword = '';
			newPassword = '';
			confirmPassword = '';
		} catch (e) {
			notify.error(e);
		} finally {
			pwLoading = false;
		}
	}

	async function createUser() {
		if (!newUsername.trim() || newUserPassword.length < 6) {
			notify.error('Username required and password must be at least 6 characters');
			return;
		}
		userLoading = true;
		try {
			await api.createUser(newUsername.trim(), newUserPassword, newUserRole);
			newUsername = '';
			newUserPassword = '';
			newUserRole = 'read';
			await refreshUsers();
			notify.success('User created');
		} catch (e) {
			notify.error(e);
		} finally {
			userLoading = false;
		}
	}

	async function updateRole(userId: string, role: string) {
		try {
			await api.updateUser(userId, { role });
			editingUser = null;
			await refreshUsers();
		} catch (e) {
			notify.error(e);
		}
	}

	async function deleteUser(userId: string) {
		try {
			await api.deleteUser(userId);
			await refreshUsers();
		} catch (e) {
			notify.error(e);
		}
	}
</script>

<div class="mx-auto max-w-2xl space-y-4">
	<!-- Change Password -->
	<section class="panel">
		<header class="panel__head">▮ Change Password</header>
		<div class="panel__body">
			<form onsubmit={(e) => { e.preventDefault(); changePassword(); }} class="space-y-3">
				<div>
					<label class="field-label" for="pw-current">Current password</label>
					<input
						id="pw-current"
						bind:value={currentPassword}
						type="password"
						autocomplete="current-password"
						class="input"
					/>
				</div>
				<div>
					<label class="field-label" for="pw-new">New password (min 6 chars)</label>
					<input
						id="pw-new"
						bind:value={newPassword}
						type="password"
						autocomplete="new-password"
						class="input"
					/>
				</div>
				<div>
					<label class="field-label" for="pw-confirm">Confirm new password</label>
					<input
						id="pw-confirm"
						bind:value={confirmPassword}
						type="password"
						autocomplete="new-password"
						class="input"
					/>
				</div>
				<button type="submit" disabled={pwLoading} class="btn">
					{pwLoading ? 'Changing…' : 'Change Password'}
				</button>
			</form>
			{#if pwSuccess}
				<p class="mt-2 text-live-glow text-xs">Password changed.</p>
			{/if}
		</div>
	</section>

	<!-- Users -->
	<section class="panel">
		<header class="panel__head">▮ Users</header>
		<div class="panel__body">
			{#if users.length > 0}
				<div class="mb-4 space-y-2">
					{#each users as user (user.id)}
						<div class="row">
							<div class="flex items-center gap-3">
								<span class="text-amber">{user.username}</span>
								{#if editingUser === user.id}
									<select
										aria-label="Role for {user.username}"
										bind:value={editRole}
										onchange={() => updateRole(user.id, editRole)}
										class="select w-auto"
									>
										<option value="admin">Admin</option>
										<option value="write">Write</option>
										<option value="read">Read</option>
									</select>
								{:else}
									<span class="pill pill--idle">{user.role}</span>
								{/if}
							</div>
							<div class="flex gap-2">
								<button
									onclick={() => {
										if (editingUser === user.id) {
											editingUser = null;
										} else {
											editingUser = user.id;
											editRole = user.role;
										}
									}}
									class="btn btn--ghost"
								>
									{editingUser === user.id ? 'Cancel' : 'Edit'}
								</button>
								<button onclick={() => deleteUser(user.id)} class="btn btn--danger">
									Delete
								</button>
							</div>
						</div>
					{/each}
				</div>
			{/if}

			<h3 class="label mb-2">Add User</h3>
			<form onsubmit={(e) => { e.preventDefault(); createUser(); }} class="space-y-2">
				<div class="flex gap-2">
					<div class="flex-1">
						<label class="field-label" for="new-username">Username</label>
						<input id="new-username" bind:value={newUsername} class="input" />
					</div>
					<div>
						<label class="field-label" for="new-role">Role</label>
						<select id="new-role" bind:value={newUserRole} class="select w-auto">
							<option value="admin">Admin</option>
							<option value="write">Write</option>
							<option value="read">Read</option>
						</select>
					</div>
				</div>
				<div>
					<label class="field-label" for="new-password">Password (min 6 chars)</label>
					<input
						id="new-password"
						bind:value={newUserPassword}
						type="password"
						autocomplete="new-password"
						class="input"
					/>
				</div>
				<button
					type="submit"
					disabled={userLoading || !newUsername.trim() || newUserPassword.length < 6}
					class="btn"
				>
					+ Add User
				</button>
			</form>

			<div class="mt-3 rounded-sm border border-border-dim bg-panel-raised p-3 text-xs text-amber-dim">
				<p class="label mb-1">Roles</p>
				<p><span class="text-amber">Admin</span> — Full access: settings, users, stream control, sources</p>
				<p><span class="text-amber">Write</span> — Stream control: go live, switch sources, manage destinations</p>
				<p><span class="text-amber">Read</span> — View only: monitor studio, see stats</p>
			</div>
		</div>
	</section>

	<!-- Program failover -->
	<section class="panel">
		<header class="panel__head">▮ Program Failover</header>
		<div class="panel__body">
			<p class="mb-3 text-xs text-amber-dim leading-relaxed">
				If the live program output goes offline (e.g. an IRL stream drops), automatically
				switch to a fallback source so your destinations don't disconnect — then switch back
				when the main source returns.
			</p>
			<div class="row mb-3">
				<div>
					<span class="text-amber">Enable failover</span>
					<p class="text-xs text-amber-dim">Auto-switch to the fallback when program goes dark.</p>
				</div>
				<button
					onclick={() => { failover.enabled = !failover.enabled; saveFailover(); }}
					class="btn {failover.enabled ? 'btn--go' : ''}"
				>
					{failover.enabled ? '● On' : '○ Off'}
				</button>
			</div>

			<label class="field-label" for="fo-src">Fallback source</label>
			<select id="fo-src" bind:value={failover.fallback_source_id} class="select mb-3 w-full">
				<option value={null}>— Select a source —</option>
				{#each sources as s}
					<option value={s.id}>{s.name}</option>
				{/each}
			</select>
			<p class="mb-3 text-[11px] text-amber-muted">
				A "be right back" image/video (upload it in the Library, add it as a source) or a backup
				live ingress.
			</p>

			<div class="grid grid-cols-2 gap-3">
				<div>
					<label class="field-label" for="fo-off">Offline after (ms)</label>
					<input id="fo-off" type="number" min="500" step="500" bind:value={failover.offline_grace_ms} class="input" />
				</div>
				<div>
					<label class="field-label" for="fo-rec">Recover after (ms)</label>
					<input id="fo-rec" type="number" min="500" step="500" bind:value={failover.recovery_grace_ms} class="input" />
				</div>
			</div>
			<button onclick={saveFailover} disabled={failoverSaving} class="btn btn--go mt-3">
				{failoverSaving ? 'Saving…' : 'Save failover settings'}
			</button>
		</div>
	</section>

	<!-- Display -->
	<section class="panel">
		<header class="panel__head">▮ Display</header>
		<div class="panel__body">
			<div class="row">
				<div>
					<span class="text-amber">Reduce effects</span>
					<p class="text-xs text-amber-dim">Disables scanlines, glow, and pulsing.</p>
				</div>
				<button onclick={toggleFx} class="btn {fxOff ? '' : 'btn--go'}">
					{fxOff ? '○ Off' : '● On'}
				</button>
			</div>
		</div>
	</section>

	<!-- API Keys -->
	<a href="/settings/keys" class="row hover:bg-panel-raised text-amber">
		<span>Manage API Keys</span>
		<span class="text-amber-muted">▸</span>
	</a>
</div>
