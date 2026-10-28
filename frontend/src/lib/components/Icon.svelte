<script lang="ts" module>
	/**
	 * Icons from Lucide (ISC licensed), inlined rather than installed.
	 *
	 * A runtime icon package would mean either a CDN request on every page or
	 * a few hundred kilobytes of paths nobody draws. These are the thirty the
	 * app actually uses, copied as path data, so they cost nothing at runtime
	 * and work with no network at all.
	 *
	 * Credit lives on the About page.
	 */
	export const ICONS = {
		home: 'M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8|M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
		grid: 'M3 3h7v7H3z|M14 3h7v7h-7z|M14 14h7v7h-7z|M3 14h7v7H3z',
		move: 'M5 9l-3 3 3 3|M9 5l3-3 3 3|M15 19l-3 3-3-3|M19 9l3 3-3 3|M2 12h20|M12 2v20',
		users:
			'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2|M9 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8z|M22 21v-2a4 4 0 0 0-3-3.87|M16 3.13a4 4 0 0 1 0 7.75',
		search: 'M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16z|m21 21-4.3-4.3',
		calendar:
			'M8 2v4|M16 2v4|M3 10h18|M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z',
		book: 'M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H19a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H6.5a1 1 0 0 1 0-5H20',
		chart: 'M3 3v16a2 2 0 0 0 2 2h16|M18 17V9|M13 17V5|M8 17v-3',
		settings:
			'M20 7h-9|M14 17H5|M17 14a3 3 0 1 0 0 6 3 3 0 0 0 0-6z|M7 4a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',
		info: 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z|M12 16v-4|M12 8h.01',
		sparkle: 'M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z|M19 17v4|M17 19h4',
		upload: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4|M17 8l-5-5-5 5|M12 3v12',
		download: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4|M7 10l5 5 5-5|M12 15V3',
		contrast: 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z|M12 18a6 6 0 0 0 0-12z',
		'eye-off':
			'M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68|M6.61 6.61A13.5 13.5 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61|M2 2l20 20|M9.9 9.9a3 3 0 0 0 4.2 4.2',
		logout: 'M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4|M16 17l5-5-5-5|M21 12H9',
		heart:
			'M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7z',
		star: 'M12 2.5l2.9 5.9 6.6.9-4.8 4.6 1.2 6.5-5.9-3.1-5.9 3.1 1.2-6.5L2.5 9.3l6.6-.9z',
		play: 'M8 5.14v13.72a1 1 0 0 0 1.5.86l11-6.86a1 1 0 0 0 0-1.72l-11-6.86A1 1 0 0 0 8 5.14z',
		check: 'M20 6 9 17l-5-5',
		plus: 'M5 12h14|M12 5v14',
		x: 'M18 6 6 18|M6 6l12 12',
		'chevron-right': 'm9 18 6-6-6-6',
		'chevron-left': 'm15 18-6-6 6-6',
		'chevron-down': 'm6 9 6 6 6-6',
		'chevron-up': 'm18 15-6-6-6 6',
		'arrow-up': 'M12 19V5|m5 12 7-7 7 7',
		'arrow-down': 'M12 5v14|m19 12-7 7-7-7',
		filter: 'M3 6h18|M7 12h10|M10 18h4',
		pencil:
			'M21.17 6.81a1 1 0 0 0-3.98-3.98L3.84 16.17a2 2 0 0 0-.5.83l-1.32 4.35a.5.5 0 0 0 .62.62l4.35-1.32a2 2 0 0 0 .83-.5z',
		trash:
			'M3 6h18|M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6|M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
		grip: 'M9 5h.01|M9 12h.01|M9 19h.01|M15 5h.01|M15 12h.01|M15 19h.01',
		external: 'M15 3h6v6|M10 14 21 3|M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6',
		refresh: 'M3 12a9 9 0 0 1 15.7-6L21 8|M21 3v5h-5|M21 12a9 9 0 0 1-15.7 6L3 16|M3 21v-5h5',
		clock: 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z|M12 6v6l4 2',
		tv: 'M7 21h10|M5 3h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z',
		film: 'M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z|M7 3v18|M17 3v18|M3 8h4|M17 8h4|M3 16h4|M17 16h4',
		more: 'M12 5h.01|M12 12h.01|M12 19h.01',
		undo: 'M3 7v6h6|M3 13a9 9 0 1 0 3-7.7L3 8',
		shield:
			'M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z|m9 12 2 2 4-4',
		bell: 'M10.27 21a2 2 0 0 0 3.46 0|M3.26 15.33A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.67C19.41 13.96 18 12.5 18 8A6 6 0 0 0 6 8c0 4.5-1.41 5.96-2.74 7.33',
		'bell-off':
			'M10.27 21a2 2 0 0 0 3.46 0|M17 17H4a1 1 0 0 1-.74-1.67C4.59 13.96 6 12.5 6 8a6 6 0 0 1 .26-1.74|m2 2 20 20|M8.67 3.01A6 6 0 0 1 18 8c0 2.69.77 4.65 1.71 6.05',
		layers:
			'M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z|M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12|M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17',
		share:
			'M18 2a3 3 0 1 0 0 6 3 3 0 0 0 0-6z|M6 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z|M18 16a3 3 0 1 0 0 6 3 3 0 0 0 0-6z|m8.59 13.51 6.83 3.98|m15.41 6.51-6.82 3.98',
		flame:
			'M8.5 14.5A2.5 2.5 0 0 0 11 12c0-1.38-.5-2-1-3-1.07-2.14-.22-4.05 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 1 1-14 0c0-1.15.43-2.29 1-3a2.5 2.5 0 0 0 2.5 2.5z',
		copy: 'M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2z|M4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2',
		link: 'M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71|M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71',
		user: 'M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2|M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0z',
		palette:
			'M12 22a1 1 0 0 1 0-20 10 9 0 0 1 10 9 5 5 0 0 1-5 5h-2.25a1.75 1.75 0 0 0-1.4 2.8l.3.4a1.75 1.75 0 0 1-1.4 2.8z|M13.5 6.5h.01|M17.5 10.5h.01|M8.5 7.5h.01|M6.5 12.5h.01',
		plug: 'M12 22v-5|M9 8V2|M15 8V2|M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8z',
		database: 'M3 5a9 3 0 1 0 18 0 9 3 0 1 0-18 0z|M3 5v14a9 3 0 0 0 18 0V5|M3 12a9 3 0 0 0 18 0',
		image:
			'M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z|M11 9a2 2 0 1 1-4 0 2 2 0 0 1 4 0z|m21 15-3.09-3.09a2 2 0 0 0-2.82 0L6 21',
		camera:
			'M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z|M15 13a3 3 0 1 1-6 0 3 3 0 0 1 6 0z'
	} as const;

	export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
	interface Props {
		name: IconName;
		size?: number;
		/** Solid rather than outlined: for play, heart-on and star-on. */
		filled?: boolean;
		strokeWidth?: number;
		label?: string;
	}

	let { name, size = 18, filled = false, strokeWidth = 1.9, label }: Props = $props();

	const paths = $derived(ICONS[name].split('|'));
</script>

<svg
	class="icon"
	viewBox="0 0 24 24"
	width={size}
	height={size}
	fill={filled ? 'currentColor' : 'none'}
	stroke={filled ? 'none' : 'currentColor'}
	stroke-width={strokeWidth}
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden={label ? undefined : 'true'}
	aria-label={label}
	role={label ? 'img' : undefined}
>
	{#each paths as d (d)}
		<path {d} />
	{/each}
</svg>

<style>
	.icon {
		display: block;
		flex: none;
	}
</style>
