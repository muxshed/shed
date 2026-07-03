// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

import { pipelineState, sources, destinations, recordingState, failover, activeSchedule } from './stores/pipeline';
import type { WsEvent } from './types';
import { notify } from '$lib/notify';

let ws: WebSocket | null = null;
let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
let reconnectDelay = 1000;
const MAX_DELAY = 30000;

export function connectWs() {
	if (typeof window === 'undefined') return;

	const token = localStorage.getItem('muxshed_session_token') || '';
	const apiKey = localStorage.getItem('muxshed_api_key') || '';
	if (!token && !apiKey) return;

	const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	const authParam = token ? `token=${token}` : `key=${apiKey}`;
	const url = `${proto}//${window.location.host}/api/v1/ws?${authParam}`;

	ws = new WebSocket(url);

	ws.onopen = () => {
		reconnectDelay = 1000;
	};

	ws.onmessage = (event) => {
		try {
			const data = JSON.parse(event.data) as WsEvent;
			handleEvent(data);
		} catch {
			// ignore malformed messages
		}
	};

	ws.onclose = () => {
		ws = null;
		scheduleReconnect();
	};

	ws.onerror = () => {
		ws?.close();
	};
}

export function disconnectWs() {
	if (reconnectTimeout) {
		clearTimeout(reconnectTimeout);
		reconnectTimeout = null;
	}
	ws?.close();
	ws = null;
}

function scheduleReconnect() {
	reconnectTimeout = setTimeout(() => {
		connectWs();
		reconnectDelay = Math.min(reconnectDelay * 2, MAX_DELAY);
	}, reconnectDelay);
}

function handleEvent(event: WsEvent) {
	switch (event.type) {
		case 'pipeline_state':
			pipelineState.set(event.payload);
			break;
		case 'source_state':
			sources.update((list) =>
				list.map((s) =>
					s.id === event.payload.id ? { ...s, state: event.payload.state } : s,
				),
			);
			break;
		case 'destination_state':
			destinations.update((list) =>
				list.map((d) =>
					d.id === event.payload.id
						? { ...d, enabled: event.payload.state === 'enabled' }
						: d,
				),
			);
			break;
		case 'recording_state':
			recordingState.set({
				recording: event.payload.recording,
				path: event.payload.path,
			});
			break;
		case 'failover_state':
			failover.set({
				active: event.payload.active,
				fallback_source_id: event.payload.fallback_source_id,
				intent_source_id: event.payload.intent_source_id,
			});
			break;
		case 'schedule_started':
			activeSchedule.set({ id: event.payload.id });
			break;
		case 'schedule_ended':
			activeSchedule.set({ id: null });
			break;
		case 'schedule_skipped':
			notify.info(`Schedule skipped: ${event.payload.reason}`);
			break;
	}
}
