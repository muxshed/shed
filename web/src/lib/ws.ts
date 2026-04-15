// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { pipelineState, sources, destinations, recordingState } from './stores/pipeline';
import type { WsEvent } from './types';

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
	}
}
