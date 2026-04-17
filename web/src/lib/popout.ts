// Licensed under the Business Source License 1.1 — see LICENSE.

const windows = new Map<string, Window>();

export function popout(section: string, width = 640, height = 480) {
	const existing = windows.get(section);
	if (existing && !existing.closed) {
		existing.focus();
		return;
	}

	const left = window.screenX + 50;
	const top = window.screenY + 50;
	const w = window.open(
		`/popout/${section}`,
		`muxshed-${section}`,
		`width=${width},height=${height},left=${left},top=${top}`,
	);

	if (w) {
		windows.set(section, w);
	}
}

export function isPopped(section: string): boolean {
	const w = windows.get(section);
	return !!w && !w.closed;
}
