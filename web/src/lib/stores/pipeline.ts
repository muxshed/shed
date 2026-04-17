// Licensed under the Business Source License 1.1 — see LICENSE.

import { writable, derived } from 'svelte/store';
import type { PipelineState, Source, Destination, Scene, RecordingState } from '../types';

export const pipelineState = writable<PipelineState>({ state: 'idle' });
export const sources = writable<Source[]>([]);
export const destinations = writable<Destination[]>([]);
export const scenes = writable<Scene[]>([]);
export const recordingState = writable<RecordingState>({ recording: false });

export const isLive = derived(pipelineState, ($s) => $s.state === 'live');
export const isTransitioning = derived(
	pipelineState,
	($s) => $s.state === 'starting' || $s.state === 'stopping',
);
export const isRecording = derived(recordingState, ($r) => $r.recording);
