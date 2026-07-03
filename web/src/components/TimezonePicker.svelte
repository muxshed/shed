<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	// Searchable timezone combobox. Stores the canonical IANA name (what the
	// backend / chrono-tz accepts) but lets the operator search human-readably
	// (e.g. "new york", "london", "berlin").
	let { value = $bindable(''), onselect }: { value?: string; onselect?: (tz: string) => void } = $props();

	// All IANA zones the runtime knows; underscores are the canonical form.
	const zones: string[] = (() => {
		try {
			return Intl.supportedValuesOf?.('timeZone') ?? [];
		} catch {
			return [];
		}
	})();

	let query = $state('');
	let open = $state(false);
	let active = $state(0);
	let inputEl = $state<HTMLInputElement | null>(null);
	// The dropdown is a fixed-position popover anchored to the input's screen
	// rect, so it isn't clipped by the settings panel's overflow.
	let rect = $state({ top: 0, left: 0, width: 0 });

	function place() {
		if (!inputEl) return;
		const r = inputEl.getBoundingClientRect();
		rect = { top: r.bottom, left: r.left, width: r.width };
	}
	function openList() {
		open = true;
		active = 0;
		place();
	}

	$effect(() => {
		if (!open) return;
		const reposition = () => place();
		window.addEventListener('scroll', reposition, true);
		window.addEventListener('resize', reposition);
		return () => {
			window.removeEventListener('scroll', reposition, true);
			window.removeEventListener('resize', reposition);
		};
	});

	function offset(tz: string): string {
		try {
			const part = new Intl.DateTimeFormat('en-US', { timeZone: tz, timeZoneName: 'shortOffset' })
				.formatToParts(new Date())
				.find((p) => p.type === 'timeZoneName');
			return part?.value ?? '';
		} catch {
			return '';
		}
	}
	const norm = (s: string) => s.toLowerCase().replace(/[_/]/g, ' ');
	const label = (tz: string) => tz.replace(/_/g, ' ');

	const filtered = $derived.by(() => {
		const q = norm(query.trim());
		const list = q ? zones.filter((z) => norm(z).includes(q)) : zones;
		return list.slice(0, 60);
	});

	function pick(tz: string) {
		value = tz;
		query = '';
		open = false;
		onselect?.(tz);
	}

	function onKeydown(e: KeyboardEvent) {
		if (!open) return;
		if (e.key === 'ArrowDown') { active = Math.min(active + 1, filtered.length - 1); e.preventDefault(); }
		else if (e.key === 'ArrowUp') { active = Math.max(active - 1, 0); e.preventDefault(); }
		else if (e.key === 'Enter' && filtered[active]) { pick(filtered[active]); e.preventDefault(); }
		else if (e.key === 'Escape') { open = false; }
	}
</script>

<div class="relative">
	<input
		bind:this={inputEl}
		class="input"
		placeholder={value ? label(value) : 'Search timezone…'}
		bind:value={query}
		onfocus={openList}
		oninput={openList}
		onkeydown={onKeydown}
		onblur={() => setTimeout(() => (open = false), 150)}
		role="combobox"
		aria-expanded={open}
		aria-controls="tz-listbox"
		aria-label="Timezone"
	/>
	{#if value && !open}
		<span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[11px] text-amber-muted">{offset(value)}</span>
	{/if}

	{#if open}
		<ul
			id="tz-listbox"
			role="listbox"
			class="fixed z-50 max-h-64 overflow-y-auto rounded border border-border bg-panel-raised shadow-lg"
			style="top: {rect.top}px; left: {rect.left}px; width: {rect.width}px;"
		>
			{#if filtered.length === 0}
				<li class="px-3 py-2 text-xs text-amber-muted">No match</li>
			{:else}
				{#each filtered as tz, i (tz)}
					<li>
						<button
							type="button"
							class="flex w-full items-center justify-between px-3 py-1.5 text-left text-[13px] hover:bg-panel {i === active ? 'bg-panel text-amber-bright' : 'text-amber-dim'} {tz === value ? 'text-amber' : ''}"
							onmousedown={() => pick(tz)}
							onmouseenter={() => (active = i)}
							role="option"
							aria-selected={tz === value}
						>
							<span>{label(tz)}</span>
							<span class="ml-3 shrink-0 text-[11px] text-amber-muted">{offset(tz)}</span>
						</button>
					</li>
				{/each}
			{/if}
		</ul>
	{/if}
</div>
