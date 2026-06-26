<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { Toaster as Sonner, type ToasterProps } from 'svelte-sonner';

	let { ...restProps }: ToasterProps = $props();
</script>

<Sonner
	position="bottom-right"
	theme="dark"
	closeButton
	duration={5000}
	{...restProps}
/>

<style>
	:global([data-sonner-toast]) {
		font-family: var(--font-mono) !important;
		font-size: 13px;
		background: var(--color-panel-raised) !important;
		border: 1px solid var(--color-border) !important;
		border-radius: var(--radius-md);
		color: var(--color-amber) !important;
		box-shadow: var(--glow-amber);
	}
	:global([data-sonner-toast][data-type='error']) {
		background: #2a0d0a !important;
		border-color: #8f2f2f !important;
		color: var(--color-danger-glow) !important;
		box-shadow: var(--glow-danger);
	}
	:global([data-sonner-toast][data-type='success']) {
		background: #0e2a14 !important;
		border-color: #2f8f44 !important;
		color: var(--color-live-glow) !important;
		box-shadow: var(--glow-live);
	}
	:global([data-sonner-toast][data-type='warning']) {
		background: #2a2607 !important;
		border-color: #7a7016 !important;
		color: var(--color-warning) !important;
		box-shadow: none;
	}
	:global([data-sonner-toast] [data-title]) {
		font-weight: 500;
		letter-spacing: 0.5px;
	}
	:global([data-sonner-toast] [data-description]) {
		color: var(--color-amber-dim);
	}
	:global([data-sonner-toast] [data-close-button]) {
		background: var(--color-panel);
		border-color: var(--color-border);
		color: var(--color-amber-dim);
	}
</style>
