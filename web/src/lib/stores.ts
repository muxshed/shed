// Licensed under the Business Source License 1.1 — see LICENSE.

import { writable, derived } from 'svelte/store';
import type {
	Instance,
	Source,
	Scene,
	Destination,
	Overlay,
	PipelineState,
	RecordingState,
} from './types';

// Active instance
export const instances = writable<Instance[]>([]);
export const activeInstanceId = writable<string | null>(null);
export const activeInstance = derived([instances, activeInstanceId], ([$instances, $id]) => {
	return $instances.find((i) => i.id === $id);
});

// Studio state (synced from instance API)
export const sources = writable<Source[]>([]);
export const scenes = writable<Scene[]>([]);
export const destinations = writable<Destination[]>([]);
export const overlays = writable<Overlay[]>([]);
export const pipelineState = writable<PipelineState>({ state: 'idle' });
export const recordingState = writable<RecordingState>({ recording: false });

// Derived state
export const isLive = derived(pipelineState, ($s) => $s.state === 'live');
export const isRecording = derived(recordingState, ($r) => $r.recording);
