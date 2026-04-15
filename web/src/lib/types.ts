// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

export type SourceKind =
	| { type: 'rtmp'; stream_key: string }
	| { type: 'srt'; port: number; passphrase?: string }
	| { type: 'web_rtc'; token: string }
	| { type: 'test_pattern' }
	| { type: 'media_file'; asset_id: string; file_path: string; loop_mode: string };

export type SourceState = 'disconnected' | 'connecting' | 'live' | { error: string };

export interface Source {
	id: string;
	name: string;
	kind: SourceKind;
	state: SourceState;
}

export type DestinationKind =
	| { type: 'rtmp'; url: string; stream_key: string }
	| { type: 'rtmps'; url: string; stream_key: string }
	| { type: 'srt'; url: string }
	| { type: 'recording'; path: string };

export interface Destination {
	id: string;
	name: string;
	kind: DestinationKind;
	enabled: boolean;
}

export type PipelineState =
	| { state: 'idle' }
	| { state: 'starting' }
	| { state: 'live'; started_at: string; active_scene: string }
	| { state: 'stopping' }
	| { state: 'error'; message: string };

export interface ApiKey {
	id: string;
	name: string;
	scopes: string[];
	created_at: string;
	last_used_at?: string;
}

export interface Layer {
	id: string;
	source_id: string;
	position: { x: number; y: number };
	size: { width: number; height: number };
	z_index: number;
	opacity: number;
}

export interface Scene {
	id: string;
	name: string;
	layers: Layer[];
}

export type OverlayKind =
	| { type: 'image'; file_path: string }
	| { type: 'lower_third'; title: string; subtitle: string; background_color: string; text_color: string };

export interface Overlay {
	id: string;
	name: string;
	kind: OverlayKind;
	position: { x: number; y: number };
	size: { width: number; height: number };
	visible: boolean;
	z_index: number;
}

export interface RecordingState {
	recording: boolean;
	path?: string;
	started_at?: string;
}

export interface BroadcastConfig {
	source_id: string | null;
	scene_id: string | null;
	start_stinger_id: string | null;
	destination_ids: string[];
	enable_delay: boolean;
	delay_ms: number;
	auto_record: boolean;
}

export interface AudioChannelState {
	source_id: string;
	muted: boolean;
	volume: number;
}

export interface AudioRouting {
	active_audio_source: string | null;
	channels: AudioChannelState[];
	audio_follows_video: boolean;
}

export interface Asset {
	id: string;
	name: string;
	asset_type: 'image' | 'video' | 'stinger';
	file_path: string;
	file_size: number;
	mime_type: string;
	folder_id: string | null;
	loop_mode: 'one_shot' | 'loop';
	duration_ms: number;
	start_ms: number;
	opaque_ms: number;
	clear_ms: number;
	end_ms: number;
	audio_behaviour: string;
	has_thumbnail: boolean;
	metadata: {
		duration_ms?: number;
		width?: number;
		height?: number;
		codec?: string;
		fps?: number;
		audio_codec?: string;
		bitrate_kbps?: number;
	};
	created_at: string;
}

export interface AssetFolder {
	id: string;
	name: string;
	parent_id: string | null;
	color: string;
	created_at: string;
}

export interface OutputConfig {
	video_bitrate_kbps: number;
	audio_bitrate_kbps: number;
	width: number;
	height: number;
	fps: number;
}

export interface OutputStats {
	bytes_sent: number;
	duration_secs: number;
	source_bitrate_kbps: number;
	output_bitrate_kbps: number;
	dropped_frames: number;
	source_width?: number;
	source_height?: number;
	source_fps?: number;
	source_encoder?: string;
}

export type StingerAudio = 'silent' | { duck: number } | 'overlay' | 'replace';

export interface StingerConfig {
	id: string;
	name: string;
	file_path: string;
	duration_ms: number;
	start_ms: number;
	opaque_ms: number;
	clear_ms: number;
	end_ms: number;
	audio_behaviour: StingerAudio;
	thumbnail_path: string;
}

export interface DelayConfig {
	enabled: boolean;
	duration_ms: number;
	whisper_enabled: boolean;
}

export interface Guest {
	id: string;
	name: string;
	created_at: string;
}

export type WsEvent =
	| { type: 'pipeline_state'; payload: PipelineState }
	| { type: 'source_state'; payload: { id: string; state: SourceState } }
	| { type: 'destination_state'; payload: { id: string; state: string } }
	| { type: 'scene_changed'; payload: { scene_id: string; method: string } }
	| { type: 'recording_state'; payload: { recording: boolean; path?: string } }
	| { type: 'transition_started'; payload: { stinger_id: string; target_scene_id: string } }
	| { type: 'transition_complete'; payload: { scene_id: string } }
	| { type: 'bleep_triggered'; payload: { at_ms: number; source: string } }
	| { type: 'error'; payload: { message: string; code: string } };
