// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { Source, SourceKind, Destination, DestinationKind, ApiKey, Scene, Layer, RecordingState, StingerConfig, StingerAudio, DelayConfig, Guest, BroadcastConfig, OutputConfig, OutputStats, AudioRouting, Asset, AssetFolder, User } from './types';

function getSessionToken(): string {
	if (typeof window === 'undefined') return '';
	return localStorage.getItem('muxshed_session_token') || '';
}

export function setSessionToken(token: string) {
	localStorage.setItem('muxshed_session_token', token);
}

export function clearSession() {
	localStorage.removeItem('muxshed_session_token');
}

export function hasSession(): boolean {
	return getSessionToken().length > 0;
}

function getApiKey(): string {
	if (typeof window === 'undefined') return '';
	return localStorage.getItem('muxshed_api_key') || '';
}

export function setApiKey(key: string) {
	localStorage.setItem('muxshed_api_key', key);
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
	const headers: Record<string, string> = {};

	if (options.body) {
		headers['Content-Type'] = 'application/json';
	}

	const token = getSessionToken();
	const apiKey = getApiKey();
	if (token) {
		headers['Authorization'] = `Bearer ${token}`;
	} else if (apiKey) {
		headers['X-API-Key'] = apiKey;
	}

	const res = await fetch(`/api/v1${path}`, {
		...options,
		headers: {
			...headers,
			...options.headers,
		},
	});

	if (!res.ok) {
		const body = await res.json().catch(() => ({ error: { message: res.statusText } }));
		throw new Error(body.error?.message || res.statusText);
	}

	if (res.status === 204) return undefined as T;
	const text = await res.text();
	if (!text) return undefined as T;
	return JSON.parse(text);
}

export const api = {
	// Auth
	login: (username: string, password: string) =>
		request<{ token: string; username: string; role: string; expires_at: string }>('/auth/login', {
			method: 'POST',
			body: JSON.stringify({ username, password }),
		}),
	logout: () => {
		const token = getSessionToken();
		return request<void>('/auth/logout', {
			method: 'POST',
			body: JSON.stringify({ token }),
		});
	},
	me: () => request<{ id: string; username: string; role: string }>('/auth/me'),
	changePassword: (currentPassword: string, newPassword: string) =>
		request<void>('/auth/change-password', {
			method: 'POST',
			body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
		}),

	// Users
	listUsers: () => request<User[]>('/users'),
	createUser: (username: string, password: string, role: string) =>
		request<User>('/users', {
			method: 'POST',
			body: JSON.stringify({ username, password, role }),
		}),
	updateUser: (id: string, data: { username?: string; password?: string; role?: string }) =>
		request<User>(`/users/${id}`, {
			method: 'PUT',
			body: JSON.stringify(data),
		}),
	deleteUser: (id: string) => request<void>(`/users/${id}`, { method: 'DELETE' }),

	// Sources
	listSources: () => request<Source[]>('/sources'),
	createSource: (name: string, kind: SourceKind) =>
		request<Source>('/sources', {
			method: 'POST',
			body: JSON.stringify({ name, kind }),
		}),
	deleteSource: (id: string) => request<void>(`/sources/${id}`, { method: 'DELETE' }),
	createSourceFromAsset: (assetId: string, name?: string) =>
		request<Source>('/sources/from-asset', {
			method: 'POST',
			body: JSON.stringify({ asset_id: assetId, name }),
		}),

	// Destinations
	listDestinations: () => request<Destination[]>('/destinations'),
	createDestination: (name: string, kind: DestinationKind) =>
		request<Destination>('/destinations', {
			method: 'POST',
			body: JSON.stringify({ name, kind }),
		}),
	updateDestination: (id: string, data: { name?: string; kind?: DestinationKind }) =>
		request<Destination>(`/destinations/${id}`, {
			method: 'PUT',
			body: JSON.stringify(data),
		}),
	deleteDestination: (id: string) => request<void>(`/destinations/${id}`, { method: 'DELETE' }),
	enableDestination: (id: string) =>
		request<void>(`/destinations/${id}/enable`, { method: 'POST' }),
	disableDestination: (id: string) =>
		request<void>(`/destinations/${id}/disable`, { method: 'POST' }),

	// Audio routing
	getAudioRouting: () => request<AudioRouting>('/audio/routing'),
	setAudioSource: (sourceId: string | null) =>
		request<void>('/audio/source', { method: 'POST', body: JSON.stringify({ source_id: sourceId }) }),
	toggleAudioFollowsVideo: () => request<void>('/audio/follows-video', { method: 'POST' }),
	muteSource: (sourceId: string) => request<void>(`/audio/mute/${sourceId}`, { method: 'POST' }),
	unmuteSource: (sourceId: string) => request<void>(`/audio/unmute/${sourceId}`, { method: 'POST' }),

	// Output config/stats
	getOutputConfig: () => request<OutputConfig>('/output/config'),
	setOutputConfig: (config: OutputConfig) =>
		request<OutputConfig>('/output/config', { method: 'PUT', body: JSON.stringify(config) }),
	getOutputStats: () => request<OutputStats>('/output/stats'),

	// Broadcast config
	getBroadcastConfig: () => request<BroadcastConfig>('/broadcast/config'),
	setBroadcastConfig: (config: BroadcastConfig) =>
		request<BroadcastConfig>('/broadcast/config', {
			method: 'PUT',
			body: JSON.stringify(config),
		}),

	// Source switching
	getProgram: () => request<{ program_source_id: string | null; preview_source_id: string | null }>('/program'),
	setPreview: (sourceId: string) => request<void>(`/preview/${sourceId}`, { method: 'POST' }),
	cutToSource: (sourceId: string) => request<void>(`/cut/${sourceId}`, { method: 'POST' }),
	autoTransition: () => request<void>('/auto', { method: 'POST' }),

	// Stream control
	startStream: (sourceId?: string) =>
		request<void>('/stream/start', {
			method: 'POST',
			...(sourceId ? { body: JSON.stringify({ source_id: sourceId }) } : {}),
		}),
	stopStream: () => request<void>('/stream/stop', { method: 'POST' }),

	// Status
	getStatus: () => request<{ pipeline: import('./types').PipelineState }>('/status'),

	// Scenes
	listScenes: () => request<Scene[]>('/scenes'),
	createScene: (name: string, layers?: { source_id: string; x?: number; y?: number; width?: number; height?: number; z_index?: number; opacity?: number }[]) =>
		request<Scene>('/scenes', {
			method: 'POST',
			body: JSON.stringify({ name, layers }),
		}),
	getScene: (id: string) => request<Scene>(`/scenes/${id}`),
	updateScene: (id: string, data: { name?: string }) =>
		request<Scene>(`/scenes/${id}`, {
			method: 'PUT',
			body: JSON.stringify(data),
		}),
	deleteScene: (id: string) => request<void>(`/scenes/${id}`, { method: 'DELETE' }),
	activateScene: (id: string) => request<void>(`/scenes/${id}/activate`, { method: 'POST' }),
	addLayer: (sceneId: string, layer: { source_id: string; x?: number; y?: number; width?: number; height?: number; z_index?: number; opacity?: number }) =>
		request<Layer>(`/scenes/${sceneId}/layers`, {
			method: 'POST',
			body: JSON.stringify(layer),
		}),
	updateLayer: (sceneId: string, layerId: string, data: { source_id?: string; x?: number; y?: number; width?: number; height?: number; z_index?: number; opacity?: number }) =>
		request<Layer>(`/scenes/${sceneId}/layers/${layerId}`, {
			method: 'PUT',
			body: JSON.stringify(data),
		}),
	deleteLayer: (sceneId: string, layerId: string) =>
		request<void>(`/scenes/${sceneId}/layers/${layerId}`, { method: 'DELETE' }),

	// Recording
	startRecording: () => request<void>('/record/start', { method: 'POST' }),
	stopRecording: () => request<void>('/record/stop', { method: 'POST' }),
	recordingStatus: () => request<RecordingState>('/record/status'),

	// Assets
	listAssets: (folderId?: string) => {
		const q = folderId ? `?folder_id=${folderId}` : '';
		return request<Asset[]>(`/assets${q}`);
	},
	getAsset: (id: string) => request<Asset>(`/assets/${id}`),
	updateAsset: (id: string, data: Partial<Asset>) =>
		request<Asset>(`/assets/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
	deleteAsset: (id: string) => request<void>(`/assets/${id}`, { method: 'DELETE' }),
	uploadAsset: async (name: string, file: File, folderId?: string): Promise<Asset> => {
		const formData = new FormData();
		formData.append('name', name);
		formData.append('file', file);
		if (folderId) formData.append('folder_id', folderId);

		const headers: Record<string, string> = {};
		const token = getSessionToken();
		const apiKey = getApiKey();
		if (token) headers['Authorization'] = `Bearer ${token}`;
		else if (apiKey) headers['X-API-Key'] = apiKey;

		const res = await fetch('/api/v1/assets/upload', {
			method: 'POST',
			headers,
			body: formData,
		});
		if (!res.ok) {
			const body = await res.json().catch(() => ({ error: { message: res.statusText } }));
			throw new Error(body.error?.message || res.statusText);
		}
		return res.json();
	},
	assetFileUrl: (id: string) => `/api/v1/assets/${id}/file`,
	assetThumbnailUrl: (id: string) => `/api/v1/assets/${id}/thumbnail`,

	// Folders
	listFolders: () => request<AssetFolder[]>('/folders'),
	createFolder: (name: string, parentId?: string, color?: string) =>
		request<AssetFolder>('/folders', { method: 'POST', body: JSON.stringify({ name, parent_id: parentId, color }) }),
	updateFolder: (id: string, data: { name?: string; color?: string }) =>
		request<AssetFolder>(`/folders/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
	deleteFolder: (id: string) => request<void>(`/folders/${id}`, { method: 'DELETE' }),

	// Library (stingers/transitions — legacy)
	listLibrary: () => request<StingerConfig[]>('/library'),
	createLibraryItem: (data: { name: string; file_path: string; duration_ms?: number; start_ms?: number; opaque_ms?: number; clear_ms?: number; end_ms?: number; audio_behaviour?: StingerAudio }) =>
		request<StingerConfig>('/library', { method: 'POST', body: JSON.stringify(data) }),
	uploadLibraryItem: async (name: string, file: File): Promise<StingerConfig> => {
		const formData = new FormData();
		formData.append('name', name);
		formData.append('file', file);

		const headers: Record<string, string> = {};
		const token = getSessionToken();
		const apiKey = getApiKey();
		if (token) headers['Authorization'] = `Bearer ${token}`;
		else if (apiKey) headers['X-API-Key'] = apiKey;

		const res = await fetch('/api/v1/library/upload', {
			method: 'POST',
			headers,
			body: formData,
		});
		if (!res.ok) {
			const body = await res.json().catch(() => ({ error: { message: res.statusText } }));
			throw new Error(body.error?.message || res.statusText);
		}
		return res.json();
	},
	getLibraryItem: (id: string) => request<StingerConfig>(`/library/${id}`),
	updateLibraryItem: (id: string, data: { name?: string; start_ms?: number; opaque_ms?: number; clear_ms?: number; end_ms?: number; audio_behaviour?: StingerAudio }) =>
		request<StingerConfig>(`/library/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
	deleteLibraryItem: (id: string) => request<void>(`/library/${id}`, { method: 'DELETE' }),
	triggerTransition: (stingerId: string, targetSceneId: string) =>
		request<void>('/transition/stinger', { method: 'POST', body: JSON.stringify({ stinger_id: stingerId, target_scene_id: targetSceneId }) }),

	// Delay / Bleep
	getDelay: () => request<DelayConfig>('/delay'),
	updateDelay: (data: { enabled?: boolean; duration_ms?: number; whisper_enabled?: boolean }) =>
		request<DelayConfig>('/delay', { method: 'PUT', body: JSON.stringify(data) }),
	enableDelay: () => request<void>('/delay/enable', { method: 'POST' }),
	disableDelay: () => request<void>('/delay/disable', { method: 'POST' }),
	triggerBleep: () => request<void>('/delay/bleep', { method: 'POST' }),

	// Guests
	inviteGuest: (name: string) =>
		request<{ id: string; name: string; token: string; url: string; created_at: string }>('/guests/invite', { method: 'POST', body: JSON.stringify({ name }) }),
	listGuests: () => request<Guest[]>('/guests'),
	deleteGuest: (id: string) => request<void>(`/guests/${id}`, { method: 'DELETE' }),

	// API Keys
	listKeys: () => request<ApiKey[]>('/keys'),
	createKey: (name: string, scopes: string[]) =>
		request<{ id: string; name: string; key: string; scopes: string[] }>('/keys', {
			method: 'POST',
			body: JSON.stringify({ name, scopes }),
		}),
	deleteKey: (id: string) => request<void>(`/keys/${id}`, { method: 'DELETE' }),
};
