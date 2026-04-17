<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import type { User } from '$lib/types';

	// Password change
	let currentPassword = $state('');
	let newPassword = $state('');
	let confirmPassword = $state('');
	let pwError = $state('');
	let pwSuccess = $state(false);
	let pwLoading = $state(false);

	// Users
	let users = $state<User[]>([]);
	let newUsername = $state('');
	let newUserPassword = $state('');
	let newUserRole = $state<'admin' | 'write' | 'read'>('read');
	let userError = $state('');
	let userLoading = $state(false);
	let editingUser = $state<string | null>(null);
	let editRole = $state('');

	onMount(async () => {
		await refreshUsers();
	});

	async function refreshUsers() {
		users = await api.listUsers();
	}

	async function changePassword() {
		pwError = '';
		pwSuccess = false;
		if (newPassword.length < 6) {
			pwError = 'Password must be at least 6 characters';
			return;
		}
		if (newPassword !== confirmPassword) {
			pwError = 'Passwords do not match';
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
			pwError = e instanceof Error ? e.message : 'Failed to change password';
		} finally {
			pwLoading = false;
		}
	}

	async function createUser() {
		userError = '';
		if (!newUsername.trim() || newUserPassword.length < 6) {
			userError = 'Username required and password must be at least 6 characters';
			return;
		}
		userLoading = true;
		try {
			await api.createUser(newUsername.trim(), newUserPassword, newUserRole);
			newUsername = '';
			newUserPassword = '';
			newUserRole = 'read';
			await refreshUsers();
		} catch (e) {
			userError = e instanceof Error ? e.message : 'Failed to create user';
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
			userError = e instanceof Error ? e.message : 'Failed to update user';
		}
	}

	async function deleteUser(userId: string) {
		try {
			await api.deleteUser(userId);
			await refreshUsers();
		} catch (e) {
			userError = e instanceof Error ? e.message : 'Failed to delete user';
		}
	}
</script>

<div class="mx-auto max-w-2xl">
	<h1 class="mb-6 text-2xl">Settings</h1>

	<!-- Change Password -->
	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Change Password</h2>
		<form onsubmit={(e) => { e.preventDefault(); changePassword(); }} class="space-y-3">
			<input
				bind:value={currentPassword}
				type="password"
				placeholder="Current password"
				autocomplete="current-password"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<input
				bind:value={newPassword}
				type="password"
				placeholder="New password (min 6 chars)"
				autocomplete="new-password"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<input
				bind:value={confirmPassword}
				type="password"
				placeholder="Confirm new password"
				autocomplete="new-password"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<button
				type="submit"
				disabled={pwLoading}
				class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
			>
				{pwLoading ? 'Changing...' : 'Change Password'}
			</button>
		</form>
		{#if pwError}
			<p class="mt-2 text-sm text-red-400">{pwError}</p>
		{/if}
		{#if pwSuccess}
			<p class="mt-2 text-sm text-green-400">Password changed.</p>
		{/if}
	</div>

	<!-- Users -->
	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Users</h2>

		{#if users.length > 0}
			<div class="mb-4 space-y-2">
				{#each users as user (user.id)}
					<div class="flex items-center justify-between rounded bg-neutral-800 px-3 py-2">
						<div class="flex items-center gap-3">
							<span class="text-sm text-white">{user.username}</span>
							{#if editingUser === user.id}
								<select
									bind:value={editRole}
									onchange={() => updateRole(user.id, editRole)}
									class="rounded border border-neutral-600 bg-neutral-700 px-2 py-0.5 text-xs text-white"
								>
									<option value="admin">Admin</option>
									<option value="write">Write</option>
									<option value="read">Read</option>
								</select>
							{:else}
								<span class="rounded bg-neutral-700 px-2 py-0.5 text-xs text-neutral-400 uppercase">{user.role}</span>
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
								class="text-xs text-neutral-400 hover:text-white"
							>
								{editingUser === user.id ? 'Cancel' : 'Edit'}
							</button>
							<button
								onclick={() => deleteUser(user.id)}
								class="text-xs text-red-400 hover:text-red-300"
							>
								Delete
							</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}

		<h3 class="mb-2 text-xs font-semibold text-neutral-500">Add User</h3>
		<form onsubmit={(e) => { e.preventDefault(); createUser(); }} class="space-y-2">
			<div class="flex gap-2">
				<input
					bind:value={newUsername}
					placeholder="Username"
					class="flex-1 rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				/>
				<select
					bind:value={newUserRole}
					class="rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white"
				>
					<option value="admin">Admin</option>
					<option value="write">Write</option>
					<option value="read">Read</option>
				</select>
			</div>
			<input
				bind:value={newUserPassword}
				type="password"
				placeholder="Password (min 6 chars)"
				autocomplete="new-password"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<button
				type="submit"
				disabled={userLoading || !newUsername.trim() || newUserPassword.length < 6}
				class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
			>
				Add User
			</button>
		</form>
		{#if userError}
			<p class="mt-2 text-sm text-red-400">{userError}</p>
		{/if}

		<div class="mt-3 rounded bg-neutral-800 p-3 text-xs text-neutral-500">
			<p class="font-medium text-neutral-400">Roles</p>
			<p><span class="text-white">Admin</span> -- Full access: settings, users, stream control, sources</p>
			<p><span class="text-white">Write</span> -- Stream control: go live, switch sources, manage destinations</p>
			<p><span class="text-white">Read</span> -- View only: monitor studio, see stats</p>
		</div>
	</div>

	<!-- API Keys -->
	<a
		href="/settings/keys"
		class="block rounded-lg border border-neutral-700 bg-neutral-900 p-4 text-sm text-neutral-300 hover:bg-neutral-800"
	>
		Manage API Keys
	</a>
</div>
