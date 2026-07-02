// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

import { writable, derived } from 'svelte/store';
import type { PipelineState, Source, Destination, Scene, RecordingState } from '../types';

export const pipelineState = writable<PipelineState>({ state: 'idle' });
export const sources = writable<Source[]>([]);
export const destinations = writable<Destination[]>([]);
export const scenes = writable<Scene[]>([]);
export const recordingState = writable<RecordingState>({ recording: false });
export const failover = writable<{
	active: boolean;
	fallback_source_id: string | null;
	intent_source_id: string | null;
}>({ active: false, fallback_source_id: null, intent_source_id: null });
export const activeSchedule = writable<{ id: string | null }>({ id: null });

export const isLive = derived(pipelineState, ($s) => $s.state === 'live');
export const isTransitioning = derived(
	pipelineState,
	($s) => $s.state === 'starting' || $s.state === 'stopping',
);
export const isRecording = derived(recordingState, ($r) => $r.recording);
