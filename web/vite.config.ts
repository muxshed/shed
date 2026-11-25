import { sveltekit } from '@sveltejs/kit/vite';
import adapter from '@sveltejs/adapter-static';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true,
			},
			// SPA fallback: dynamic routes (e.g. /watch/[token]) can't be prerendered.
			// The Rust server serves index.html for unknown paths, so client routing handles them.
			adapter: adapter({ fallback: 'index.html' }),
			alias: { $lib: 'src/lib' },
		}),
	],
	server: {
		proxy: {
			'/api': {
				target: 'http://localhost:8080',
				ws: true,
			},
		},
	},
});
