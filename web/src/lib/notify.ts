// Licensed under the Business Source License 1.1 — see LICENSE.

import { toast } from 'svelte-sonner';

function message(e: unknown): string {
	if (e instanceof Error) return e.message;
	if (typeof e === 'string') return e;
	return 'Something went wrong';
}

/**
 * Central user-facing notifications. All operator-triggered errors should surface
 * here as a toast rather than inline text — see DESIGN.md §8.10.
 */
export const notify = {
	error: (e: unknown) => toast.error(message(e)),
	success: (m: string) => toast.success(m),
	info: (m: string) => toast(m),
	warning: (m: string) => toast.warning(m),
};
