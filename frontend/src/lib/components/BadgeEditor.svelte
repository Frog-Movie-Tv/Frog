<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { api } from '#lib/api';
	import { removeBackground, type Rgb } from '#lib/badge-fill';
	import {
		base64,
		clampCrop,
		halve,
		LARGE,
		PREVIEW,
		readSource,
		render,
		restoreSettings,
		SHARP_PIXELS,
		SMALL,
		startingSettings,
		startingStrength,
		toPng,
		workerFiller,
		colourAt,
		type BadgeSettings,
		type Fill,
		type Filler,
		type Source
	} from '#lib/badge';
	import { backCloses } from '#lib/layers.svelte';
	import {
		COLOUR_NAMES,
		PALETTE,
		PROFILE_COLOURS,
		PROFILE_SHADES,
		SHADE_NAMES,
		type ProfileShade
	} from '#lib/profile-colours';
	import { toasts } from '#lib/toast.svelte';
	import type { AdminBadge } from '#lib/types';
	import Icon from './Icon.svelte';
	import Spinner from './Spinner.svelte';

	interface Props {
		/** The badge being edited, or nothing for a new one. */
		badge?: AdminBadge | null;
		onclose: () => void;
		onsaved: (id: number) => void;
	}

	let { badge = null, onclose, onsaved }: Props = $props();

	const NAME_MAX = 32;
	const DESCRIPTION_MAX = 140;

	// The editor opens afresh for each badge, so it starts from what it was given.
	let name = $state(untrack(() => badge?.name ?? ''));
	let description = $state(untrack(() => badge?.description ?? ''));
	let source = $state.raw<Source | null>(null);
	let settings = $state<BadgeSettings | null>(null);
	let reading = $state(false);
	let saving = $state(false);
	/** What a tap on the picture does, while the background is being taken out. */
	let tool = $state<'move' | 'pick' | 'spot' | 'erase' | 'restore'>('move');
	/** The brush, as a share of the crop: small for an edge, large for a patch. */
	let brush = $state(5);
	/** Where the brush is over the frame, to draw its circle. */
	let hover = $state<{ x: number; y: number } | null>(null);
	let painting = false;
	/** Where the ink is: it trails the pointer, which evens out a shaky hand. */
	let ink: [number, number] | null = null;
	/** How quickly the ink catches up with the pointer: lower is steadier. */
	const FOLLOW = 0.3;
	const brushing = $derived(tool === 'erase' || tool === 'restore');

	/*
	 * Touching up magnifies the finished badge rather than cropping it: the
	 * crop and the fill stay as they were set, and the view moves over them
	 * (a shift and a scale of the frame's contents, in frame pixels).
	 */
	let view = $state({ scale: 1, x: 0, y: 0 });
	const MAGNIFY = 8;
	$effect(() => {
		if (!brushing) view = { scale: 1, x: 0, y: 0 };
	});

	/** Keep the magnified badge covering the frame. */
	function placeView(scale: number, x: number, y: number) {
		const s = Math.min(MAGNIFY, Math.max(1, scale));
		const least = frameSize - frameSize * s;
		view = {
			scale: s,
			x: Math.min(0, Math.max(least, x)),
			y: Math.min(0, Math.max(least, y))
		};
	}

	/** Magnify to `scale`, keeping the frame point (px, py) where it is. */
	function magnify(scale: number, px = frameSize / 2, py = frameSize / 2) {
		const s = Math.min(MAGNIFY, Math.max(1, scale));
		placeView(s, px - ((px - view.x) * s) / view.scale, py - ((py - view.y) * s) / view.scale);
	}

	/** A view gesture under way: two fingers, Shift or the right button. */
	let looking: {
		x: number;
		y: number;
		spread: number;
		from: { scale: number; x: number; y: number };
	} | null = null;
	let found = $state<Rgb | null>(null);
	let previewLarge = $state<string | null>(null);
	let previewSmall = $state<string | null>(null);
	let frameSize = $state(260);

	backCloses(
		() => true,
		() => onclose()
	);

	// --- the fill, off the main thread ---------------------------------
	let worker: Worker | null = null;
	let filler: Filler = async (data, width, height, options) => ({
		data,
		seed: removeBackground(data, width, height, options)
	});

	onMount(() => {
		try {
			worker = new Worker(new URL('../badge-fill.worker.ts', import.meta.url), {
				type: 'module'
			});
			filler = workerFiller(worker);
		} catch {
			// The page keeps going on its own thread: slower, never stuck.
		}
		if (badge) void reopen(badge);
	});

	onDestroy(() => {
		clearTimeout(settle);
		worker?.terminate();
		for (const url of [previewLarge, previewSmall, source?.url]) if (url) URL.revokeObjectURL(url);
	});

	async function reopen(b: AdminBadge) {
		reading = true;
		try {
			const [{ settings: saved }, original] = await Promise.all([
				api.badge(b.id),
				fetch(api.badgeOriginal(b.id)).then((r) => {
					if (!r.ok) throw new Error('That badge could not be opened.');
					return r.blob();
				})
			]);
			const s = await readSource(original);
			source = s;
			settings = restoreSettings(saved, s.width, s.height);
		} catch (e) {
			toasts.error(e);
		} finally {
			reading = false;
		}
	}

	async function choose(file: File | undefined) {
		if (!file) return;
		reading = true;
		try {
			const s = await readSource(file);
			if (source) URL.revokeObjectURL(source.url);
			source = s;
			settings = startingSettings(s.width, s.height);
			if (!name.trim()) name = file.name.replace(/\.[^.]+$/, '').slice(0, NAME_MAX);
		} catch (e) {
			toasts.error(e);
		} finally {
			reading = false;
		}
	}

	// --- the preview -----------------------------------------------------
	/*
	 * The finished badge is drawn into the frame's circle as fast as the
	 * fill keeps up: while one drawing is under way, the next waits and then
	 * takes whatever the settings are by then, so sliders feel live without a
	 * queue of stale drawings. The sizes beside it follow once it settles.
	 */
	let frameCanvas: HTMLCanvasElement | undefined = $state();
	/** The crop the frame's drawing was made for; moved since, the raw picture shows. */
	let drawnCrop = $state('');
	let drawing = false;
	let again = false;
	let ticket = 0;
	let settle: ReturnType<typeof setTimeout> | undefined;

	async function draw() {
		if (drawing) {
			again = true;
			return;
		}
		drawing = true;
		try {
			do {
				again = false;
				const src = source;
				const s = settings && ($state.snapshot(settings) as BadgeSettings);
				if (!src || !s) break;
				const mine = ++ticket;
				const { canvas, seed } = await render(src, s, brushing ? LARGE : PREVIEW, filler);
				if (mine !== ticket || src !== source) continue;
				found = seed;
				const ctx = frameCanvas?.getContext('2d');
				if (ctx && frameCanvas) {
					ctx.clearRect(0, 0, frameCanvas.width, frameCanvas.height);
					ctx.drawImage(canvas, 0, 0, frameCanvas.width, frameCanvas.height);
					drawnCrop = JSON.stringify(s.crop);
				}
				clearTimeout(settle);
				settle = setTimeout(() => void sizes(canvas, mine), 160);
			} while (again);
		} catch (e) {
			toasts.error(e);
		} finally {
			drawing = false;
		}
	}

	/** The badge at the sizes it is seen at, once the drawing has settled. */
	async function sizes(canvas: HTMLCanvasElement, mine: number) {
		const [large, small] = await Promise.all([toPng(canvas), toPng(halve(canvas, SMALL))]);
		if (mine !== ticket) return;
		for (const url of [previewLarge, previewSmall]) if (url) URL.revokeObjectURL(url);
		previewLarge = URL.createObjectURL(large);
		previewSmall = URL.createObjectURL(small);
	}

	$effect(() => {
		// Read deeply so any change to the settings draws again.
		void (settings && $state.snapshot(settings));
		void source;
		void frameCanvas;
		// Touching up draws at full size, which also clears the frame's drawing.
		void brushing;
		void draw();
	});
	const finished = $derived(!!settings && drawnCrop === JSON.stringify(settings.crop));

	// --- the frame: drag, pinch, wheel and keys ---------------------------
	const k = $derived(settings && source ? frameSize / settings.crop.size : 1);
	const soft = $derived(!!settings && settings.crop.size < SHARP_PIXELS);
	const base = $derived(source ? Math.min(source.width, source.height) : 1);
	const zoom = $derived(settings ? base / settings.crop.size : 1);

	function setCrop(crop: BadgeSettings['crop']) {
		if (!settings || !source) return;
		settings.crop = clampCrop(crop, source.width, source.height);
	}

	/** Zoom to `next`, keeping the spot at (fx, fy) of the frame where it is. */
	function zoomTo(next: number, fx = 0.5, fy = 0.5) {
		if (!settings) return;
		const c = settings.crop;
		const size = base / Math.min(8, Math.max(0.6, next));
		setCrop({ x: c.x + (c.size - size) * fx, y: c.y + (c.size - size) * fy, size });
	}

	// Fingers on the frame, read on each move; nothing draws from it.
	// eslint-disable-next-line svelte/prefer-svelte-reactivity
	const pointers = new Map<number, { x: number; y: number }>();
	let gesture: {
		crop: BadgeSettings['crop'];
		x: number;
		y: number;
		spread: number;
		moved: number;
	} | null = null;

	const spread = () => {
		const [a, b] = [...pointers.values()];
		return a && b ? Math.hypot(a.x - b.x, a.y - b.y) : 0;
	};
	const centre = () => {
		const all = [...pointers.values()];
		return {
			x: all.reduce((s, p) => s + p.x, 0) / all.length,
			y: all.reduce((s, p) => s + p.y, 0) / all.length
		};
	};

	/** A spot on the frame as a fraction of the picture, kept to three decimals. */
	function onPicture(event: PointerEvent): [number, number] | null {
		if (!settings || !source) return null;
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const fx = (event.clientX - box.left - view.x) / view.scale / box.width;
		const fy = (event.clientY - box.top - view.y) / view.scale / box.height;
		const x = settings.crop.x + fx * settings.crop.size;
		const y = settings.crop.y + fy * settings.crop.size;
		const round = (n: number) => Math.round(n * 1000) / 1000;
		return [round(x / source.width), round(y / source.height)];
	}

	function down(event: PointerEvent) {
		if (!settings) return;
		try {
			(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		} catch {
			// A pointer the browser no longer knows; the gesture works without it.
		}
		if (brushing) {
			event.preventDefault();
			pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
			const viewing =
				pointers.size >= 2 || event.shiftKey || event.button === 1 || event.button === 2;
			if (viewing) {
				// A second finger turns a stroke just begun into a pinch.
				if (painting && settings) {
					const stroke = settings.strokes.at(-1);
					if (stroke && stroke.points.length < 4) settings.strokes = settings.strokes.slice(0, -1);
					painting = false;
				}
				const c = centre();
				looking = { x: c.x, y: c.y, spread: spread(), from: { ...view } };
				return;
			}
			const at = onPicture(event);
			if (!at) return;
			painting = true;
			ink = at;
			settings.strokes = [
				...settings.strokes,
				{
					mode: tool as 'erase' | 'restore',
					width: Math.round((brush / 100) * settings.crop.size * 10) / 10,
					points: [at]
				}
			];
			return;
		}
		pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		const c = centre();
		gesture = {
			crop: { ...settings.crop },
			x: c.x,
			y: c.y,
			spread: spread(),
			moved: gesture?.moved ?? 0
		};
		if (pointers.size === 1) gesture.moved = 0;
	}

	function move(event: PointerEvent) {
		if (brushing) {
			const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
			hover = { x: event.clientX - box.left, y: event.clientY - box.top };
		}
		if (looking && pointers.has(event.pointerId)) {
			pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
			const c = centre();
			const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
			const from = looking.from;
			const scale =
				pointers.size >= 2 && looking.spread > 0
					? Math.min(MAGNIFY, Math.max(1, (from.scale * spread()) / looking.spread))
					: from.scale;
			// Zoom about where the fingers began, and move with them.
			const px = looking.x - box.left;
			const py = looking.y - box.top;
			placeView(
				scale,
				px - ((px - from.x) * scale) / from.scale + (c.x - looking.x),
				py - ((py - from.y) * scale) / from.scale + (c.y - looking.y)
			);
			return;
		}
		if (painting && settings && source && pointers.has(event.pointerId)) {
			const target = onPicture(event);
			const stroke = settings.strokes.at(-1);
			if (!target || !stroke || !ink) return;
			ink = [ink[0] + (target[0] - ink[0]) * FOLLOW, ink[1] + (target[1] - ink[1]) * FOLLOW];
			const round = (n: number) => Math.round(n * 1000) / 1000;
			const at: [number, number] = [round(ink[0]), round(ink[1])];
			const last = stroke.points.at(-1);
			// A point every quarter of the brush is smooth enough, and keeps
			// what is saved small.
			const gap = stroke.width / 4 / Math.max(source.width, source.height);
			if (!last || Math.hypot(at[0] - last[0], at[1] - last[1]) >= gap)
				stroke.points = [...stroke.points, at];
			return;
		}
		if (!gesture || !pointers.has(event.pointerId)) return;
		pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		const c = centre();
		const dx = c.x - gesture.x;
		const dy = c.y - gesture.y;
		gesture.moved = Math.max(gesture.moved, Math.hypot(dx, dy));
		const from = gesture.crop;
		const scale = frameSize / from.size;
		let size = from.size;
		if (pointers.size >= 2 && gesture.spread > 0) size = from.size * (gesture.spread / spread());
		// Pinching zooms about the middle of the fingers; moving them pans.
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const fx = (gesture.x - box.left) / box.width;
		const fy = (gesture.y - box.top) / box.height;
		setCrop({
			x: from.x + (from.size - size) * fx - dx / scale,
			y: from.y + (from.size - size) * fy - dy / scale,
			size
		});
	}

	function up(event: PointerEvent) {
		if (looking) {
			pointers.delete(event.pointerId);
			if (pointers.size === 0) looking = null;
			else {
				const c = centre();
				looking = { x: c.x, y: c.y, spread: spread(), from: { ...view } };
			}
			return;
		}
		if (painting) {
			painting = false;
			pointers.delete(event.pointerId);
			// The ink catches up where the pointer let go, so a stroke ends there.
			const target = onPicture(event);
			const stroke = settings?.strokes.at(-1);
			if (target && stroke && ink) {
				const tail: [number, number][] = [];
				for (let k = 0; k < 4; k++) {
					ink = [ink[0] + (target[0] - ink[0]) * 0.5, ink[1] + (target[1] - ink[1]) * 0.5];
					tail.push([Math.round(ink[0] * 1000) / 1000, Math.round(ink[1] * 1000) / 1000]);
				}
				stroke.points = [...stroke.points, ...tail, target];
			}
			ink = null;
			return;
		}
		if (!pointers.has(event.pointerId)) return;
		const tapped = pointers.size === 1 && (gesture?.moved ?? 0) < 5 && event.type === 'pointerup';
		pointers.delete(event.pointerId);
		if (tapped) tap(event);
		if (pointers.size && settings) {
			const c = centre();
			gesture = { crop: { ...settings.crop }, x: c.x, y: c.y, spread: spread(), moved: 99 };
		} else gesture = null;
	}

	/** A tap: pick the background, or clear a spot the border did not reach. */
	function tap(event: PointerEvent) {
		if (!settings || !source || tool === 'move' || !settings.removal.on) return;
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const x = settings.crop.x + ((event.clientX - box.left) / box.width) * settings.crop.size;
		const y = settings.crop.y + ((event.clientY - box.top) / box.height) * settings.crop.size;
		if (x < 0 || y < 0 || x >= source.width || y >= source.height) return;
		if (tool === 'pick') {
			settings.removal.seed = colourAt(source, x, y);
		} else {
			settings.removal.spots = [
				...settings.removal.spots,
				{ x: x / source.width, y: y / source.height }
			];
		}
	}

	function wheel(event: WheelEvent) {
		if (!settings) return;
		event.preventDefault();
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		if (brushing) {
			magnify(
				view.scale * Math.exp(-event.deltaY * 0.0015),
				event.clientX - box.left,
				event.clientY - box.top
			);
			return;
		}
		zoomTo(
			zoom * Math.exp(-event.deltaY * 0.0015),
			(event.clientX - box.left) / box.width,
			(event.clientY - box.top) / box.height
		);
	}

	function key(event: KeyboardEvent) {
		if (!settings) return;
		const step = settings.crop.size * (event.shiftKey ? 0.1 : 0.02);
		const c = settings.crop;
		const moves: Record<string, [number, number]> = {
			ArrowLeft: [-step, 0],
			ArrowRight: [step, 0],
			ArrowUp: [0, -step],
			ArrowDown: [0, step]
		};
		if (moves[event.key]) {
			setCrop({ ...c, x: c.x + moves[event.key][0], y: c.y + moves[event.key][1] });
		} else if (event.key === '+' || event.key === '=') {
			zoomTo(zoom * 1.1);
		} else if (event.key === '-' || event.key === '_') {
			zoomTo(zoom / 1.1);
		} else return;
		event.preventDefault();
	}

	/**
	 * Turning the background off and on again, or starting over: on, it starts
	 * at a strength measured from the picture, from the edge's own colour.
	 */
	function switchRemoval(on: boolean, fresh = false) {
		if (!settings || !source) return;
		settings.removal.on = on;
		if (!on) return;
		if (fresh || !settings.removal.seed) {
			settings.removal.seed = null;
			settings.removal.spots = [];
		}
		settings.removal.tolerance = startingStrength(
			source,
			$state.snapshot(settings) as BadgeSettings
		);
		if (fresh) {
			settings.removal.trim = 0;
			settings.removal.smooth = 1;
			settings.removal.seal = true;
		}
	}

	// --- fill and ring -----------------------------------------------------
	const FILLS: { id: Fill; label: string }[] = [
		{ id: 'none', label: 'No fill' },
		{ id: 'white', label: 'White' },
		...PROFILE_COLOURS.map((c) => ({ id: c as Fill, label: COLOUR_NAMES[c] }))
	];
	const swatch = (f: Fill, shade: ProfileShade) =>
		f === 'none' ? 'transparent' : f === 'white' ? '#ffffff' : PALETTE[f][shade].dark;
	const hex = (c: Rgb | null) =>
		c ? `#${c.map((n) => n.toString(16).padStart(2, '0')).join('')}` : 'transparent';

	// --- saving --------------------------------------------------------------
	const ready = $derived(!!source && !!settings && !!name.trim() && !saving);

	async function save() {
		if (!source || !settings) return;
		saving = true;
		try {
			const s = $state.snapshot(settings) as BadgeSettings;
			const { canvas } = await render(source, s, LARGE, filler);
			const small = halve(canvas, SMALL);
			const [large, little, original] = await Promise.all([
				toPng(canvas).then(base64),
				toPng(small).then(base64),
				base64(source.original)
			]);
			const body = {
				name: name.trim(),
				description: description.trim(),
				settings: s as unknown as Record<string, unknown>,
				large,
				small: little,
				original
			};
			const { id } = badge ? await api.updateBadge(badge.id, body) : await api.createBadge(body);
			toasts.push(badge ? 'Badge saved' : 'Badge made');
			onsaved(id);
		} catch (e) {
			toasts.error(e);
		} finally {
			saving = false;
		}
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={onclose}></div>
<div class="editor card" role="dialog" aria-modal="true" aria-labelledby="badge-editor-title">
	<header>
		<h2 id="badge-editor-title">{badge ? 'Edit badge' : 'New badge'}</h2>
		<button class="btn ghost small" aria-label="Close" onclick={onclose}>
			<Icon name="x" size={16} />
		</button>
	</header>

	<div class="body">
		<div class="left">
			{#if settings && source}
				<div
					class="frame"
					class:tapping={(tool === 'pick' || tool === 'spot') && settings.removal.on}
					class:brushing
					bind:clientWidth={frameSize}
					role="slider"
					tabindex="0"
					aria-label="Badge crop. Drag to move, arrow keys to nudge, plus and minus to zoom"
					aria-valuemin={0.6}
					aria-valuemax={8}
					aria-valuenow={Math.round(zoom * 100) / 100}
					onpointerdown={down}
					onpointermove={move}
					onpointerup={up}
					onpointercancel={up}
					onpointerleave={() => (hover = null)}
					onwheel={wheel}
					onkeydown={key}
					oncontextmenu={(e) => brushing && e.preventDefault()}
				>
					<div
						class="stage"
						style:transform="translate({view.x}px, {view.y}px) scale({view.scale})"
					>
						<img
							src={source.url}
							alt=""
							draggable="false"
							style:width="{source.width * k}px"
							style:height="{source.height * k}px"
							style:left="{-settings.crop.x * k}px"
							style:top="{-settings.crop.y * k}px"
						/>
						<!-- The finished badge, over the picture, while the crop holds still. -->
						<canvas
							class="finished"
							class:shown={finished}
							bind:this={frameCanvas}
							width={brushing ? LARGE : PREVIEW}
							height={brushing ? LARGE : PREVIEW}
							aria-hidden="true"
						></canvas>
						<span class="hole" aria-hidden="true"></span>
					</div>
					{#if brushing && hover}
						<span
							class="brush"
							aria-hidden="true"
							style:width="{(brush / 100) * frameSize * view.scale}px"
							style:height="{(brush / 100) * frameSize * view.scale}px"
							style:left="{hover.x}px"
							style:top="{hover.y}px"
						></span>
					{/if}
				</div>
				{#if brushing}
					<label class="zoom">
						<span>Magnify <em class="faint">{Math.round(view.scale * 10) / 10}×</em></span>
						<input
							type="range"
							min="1"
							max={MAGNIFY}
							step="0.1"
							value={view.scale}
							oninput={(e) => magnify(Number(e.currentTarget.value))}
						/>
					</label>
					<p class="faint small hint-move">
						Scroll or pinch to magnify. Move with two fingers, or drag with Shift or the right
						button.
					</p>
				{:else}
					<label class="zoom">
						<span>Zoom</span>
						<input
							type="range"
							min="0.6"
							max="8"
							step="0.01"
							value={zoom}
							oninput={(e) => zoomTo(Number(e.currentTarget.value))}
						/>
					</label>
				{/if}
				{#if soft}
					<p class="note">This picture is small, so it will look soft when opened.</p>
				{/if}
				<label class="btn ghost small file">
					<Icon name="image" size={15} /> Another picture
					<input
						type="file"
						accept="image/png,image/jpeg,image/webp,image/svg+xml,image/gif"
						onchange={(e) => {
							void choose(e.currentTarget.files?.[0]);
							e.currentTarget.value = '';
						}}
					/>
				</label>
			{:else if reading}
				<div class="empty-frame"><Spinner label="Reading the picture" /></div>
			{:else}
				<label class="pick">
					<Icon name="image" size={28} />
					<strong>Choose a picture</strong>
					<span class="faint">PNG, JPEG, WebP or SVG</span>
					<input
						type="file"
						accept="image/png,image/jpeg,image/webp,image/svg+xml,image/gif"
						onchange={(e) => {
							void choose(e.currentTarget.files?.[0]);
							e.currentTarget.value = '';
						}}
					/>
				</label>
			{/if}
		</div>

		<div class="right">
			<label class="field">
				<span>Name</span>
				<input class="input" bind:value={name} maxlength={NAME_MAX} placeholder="Badge name" />
			</label>
			<label class="field">
				<span>Description <em class="faint">optional</em></span>
				<input
					class="input"
					bind:value={description}
					maxlength={DESCRIPTION_MAX}
					placeholder="One line about it"
				/>
			</label>

			{#if settings}
				<section class="group">
					<label class="row">
						<strong>Remove the background</strong>
						<input
							type="checkbox"
							class="toggle-switch"
							checked={settings.removal.on}
							onchange={(e) => switchRemoval(e.currentTarget.checked)}
						/>
					</label>
					{#if settings.removal.on}
						<label class="slider">
							<span>Strength <em class="faint">{settings.removal.tolerance}</em></span>
							<input
								type="range"
								min="0"
								max="40"
								step="1"
								bind:value={settings.removal.tolerance}
							/>
						</label>
						<p class="faint small hint">
							Raise it until the background is gone; lower it if parts of the picture go too.
						</p>
						<div class="field-row">
							<span class="small">Trim edge</span>
							<div class="segmented" role="radiogroup" aria-label="Trim edge">
								{#each [[0, 'Off'], [1, 'A little'], [2, 'More'], [3, 'Most']] as [px, label] (px)}
									<button
										class:on={settings.removal.trim === px}
										role="radio"
										aria-checked={settings.removal.trim === px}
										onclick={() => settings && (settings.removal.trim = px as number)}
										>{label}</button
									>
								{/each}
							</div>
						</div>
						<div class="field-row">
							<span class="small">Smooth outline</span>
							<div class="segmented" role="radiogroup" aria-label="Smooth outline">
								{#each [[0, 'Off'], [1, 'A little'], [2, 'More']] as [level, label] (level)}
									<button
										class:on={(settings.removal.smooth ?? 1) === level}
										role="radio"
										aria-checked={(settings.removal.smooth ?? 1) === level}
										onclick={() => settings && (settings.removal.smooth = level as number)}
										>{label}</button
									>
								{/each}
							</div>
						</div>
						<label class="row">
							<span class="small"
								>Close small gaps
								<em class="faint">so the background cannot leak in through an outline</em></span
							>
							<input
								type="checkbox"
								class="toggle-switch"
								checked={settings.removal.seal !== false}
								onchange={(e) => settings && (settings.removal.seal = e.currentTarget.checked)}
							/>
						</label>
						<div class="tools segmented" role="group" aria-label="A tap on the picture">
							<button
								class:on={tool === 'move'}
								aria-pressed={tool === 'move'}
								onclick={() => (tool = 'move')}
							>
								Move
							</button>
							<button
								class:on={tool === 'pick'}
								aria-pressed={tool === 'pick'}
								onclick={() => (tool = 'pick')}
							>
								Pick background
							</button>
							<button
								class:on={tool === 'spot'}
								aria-pressed={tool === 'spot'}
								onclick={() => (tool = 'spot')}
							>
								Clear a spot
							</button>
						</div>
						{#if tool === 'pick' || tool === 'spot'}
							<p class="faint small hint">
								{tool === 'pick'
									? 'Tap the background in the picture to take that colour out.'
									: 'Tap a patch the background left behind, like the inside of a letter.'}
							</p>
						{/if}
						<p class="faint small seed">
							<span class="dot" style:background={hex(settings.removal.seed ?? found)}></span>
							{settings.removal.seed ? 'Background picked' : 'Background from the edge'}
							{#if settings.removal.spots.length}
								· {settings.removal.spots.length}
								{settings.removal.spots.length === 1 ? 'spot' : 'spots'} cleared
							{/if}
							<button class="link" onclick={() => switchRemoval(true, true)}>Start over</button>
						</p>
					{/if}
				</section>

				<section class="group">
					<strong>Touch up</strong>
					<p class="faint small hint">
						Paint over what the fill missed: Erase clears it, Restore brings the picture back.
					</p>
					<div class="tools segmented" role="group" aria-label="Brush">
						<button
							class:on={tool === 'erase'}
							aria-pressed={tool === 'erase'}
							onclick={() => (tool = tool === 'erase' ? 'move' : 'erase')}>Erase</button
						>
						<button
							class:on={tool === 'restore'}
							aria-pressed={tool === 'restore'}
							onclick={() => (tool = tool === 'restore' ? 'move' : 'restore')}>Restore</button
						>
					</div>
					{#if brushing}
						<label class="slider">
							<span>Brush size <em class="faint">{brush}</em></span>
							<input type="range" min="1" max="20" step="1" bind:value={brush} />
						</label>
					{/if}
					{#if settings.strokes.length}
						<p class="faint small seed">
							{settings.strokes.length}
							{settings.strokes.length === 1 ? 'stroke' : 'strokes'}
							<button
								class="link"
								onclick={() => settings && (settings.strokes = settings.strokes.slice(0, -1))}
								>Undo stroke</button
							>
							<button class="link" onclick={() => settings && (settings.strokes = [])}
								>Clear all</button
							>
						</p>
					{/if}
				</section>

				<section class="group">
					<strong>Circle</strong>
					<div class="swatches" role="radiogroup" aria-label="Circle fill">
						{#each FILLS as f (f.id)}
							<button
								class="swatch"
								class:none={f.id === 'none'}
								class:on={settings.fill === f.id}
								role="radio"
								aria-checked={settings.fill === f.id}
								aria-label={f.label}
								title={f.label}
								style:background={swatch(f.id, settings.shade)}
								onclick={() => settings && (settings.fill = f.id)}
							></button>
						{/each}
					</div>
					{#if settings.fill !== 'none' && settings.fill !== 'white'}
						<div class="segmented shades" role="radiogroup" aria-label="Shade">
							{#each PROFILE_SHADES as shade (shade)}
								<button
									class:on={settings.shade === shade}
									role="radio"
									aria-checked={settings.shade === shade}
									onclick={() => settings && (settings.shade = shade)}>{SHADE_NAMES[shade]}</button
								>
							{/each}
						</div>
					{/if}
					<label class="row">
						<span>A thin ring</span>
						<input type="checkbox" class="toggle-switch" bind:checked={settings.ring} />
					</label>
				</section>

				<section class="group">
					<strong>Preview</strong>
					<div class="previews">
						{#each ['light', 'dark'] as tone (tone)}
							<div class="tone" data-tone={tone}>
								{#if previewSmall && previewLarge}
									<span class="as-name">
										<b>{name.trim() || 'Badge'}</b>
										<img src={previewSmall} alt="" width="26" height="26" />
									</span>
									<span class="as-card">
										<span class="face" aria-hidden="true"></span>
										<span class="lines">
											<b>Someone</b>
											<img src={previewSmall} alt="" width="18" height="18" />
										</span>
									</span>
									<img class="as-open" src={previewLarge} alt="" width="132" height="132" />
								{:else}
									<Spinner label="Drawing the preview" />
								{/if}
							</div>
						{/each}
					</div>
				</section>
			{/if}
		</div>
	</div>

	<footer>
		<button class="btn ghost" onclick={onclose}>Cancel</button>
		<button class="btn primary" disabled={!ready} onclick={save}>
			{saving ? 'Saving…' : badge ? 'Save badge' : 'Make badge'}
		</button>
	</footer>
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 300;
		background: rgb(0 0 0 / 0.5);
	}

	.editor {
		position: fixed;
		z-index: 301;
		inset: 5vh 50% auto auto;
		transform: translateX(50%);
		width: min(940px, calc(100vw - 32px));
		max-height: 90vh;
		display: flex;
		flex-direction: column;
		padding: 0;
		background: var(--bg-float);
		overflow: hidden;
	}

	header,
	footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 14px 18px;
	}

	header {
		border-bottom: 1px solid var(--line);
	}

	header h2 {
		margin: 0;
		font-size: 1.1rem;
	}

	footer {
		justify-content: flex-end;
		border-top: 1px solid var(--line);
	}

	.body {
		display: grid;
		grid-template-columns: minmax(0, 300px) minmax(0, 1fr);
		gap: 24px;
		padding: 18px;
		overflow-y: auto;
		min-height: 0;
	}

	.left {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}

	.right {
		display: flex;
		flex-direction: column;
		gap: 16px;
		min-width: 0;
	}

	/* The picture under a round hole: what is outside the circle is dimmed,
	   and a checkerboard shows what is clear. */
	.frame {
		position: relative;
		width: min(260px, 100%);
		aspect-ratio: 1;
		overflow: hidden;
		border-radius: var(--radius);
		cursor: grab;
		touch-action: none;
		user-select: none;
		background-color: #e8e8ee;
		background-image:
			linear-gradient(45deg, #c9c9d2 25%, transparent 25%, transparent 75%, #c9c9d2 75%),
			linear-gradient(45deg, #c9c9d2 25%, transparent 25%, transparent 75%, #c9c9d2 75%);
		background-size: 16px 16px;
		background-position:
			0 0,
			8px 8px;
	}

	.frame:active {
		cursor: grabbing;
	}

	.frame.tapping {
		cursor: crosshair;
	}

	.frame.brushing {
		cursor: none;
	}

	/* What the view moves and magnifies; the crop under it stays put. */
	.stage {
		position: absolute;
		inset: 0;
		transform-origin: 0 0;
	}

	.hint-move {
		max-width: 260px;
		margin: 0;
		text-align: center;
	}

	/* The brush's reach, following the pointer. */
	.brush {
		position: absolute;
		border-radius: 50%;
		border: 1.5px solid #fff;
		box-shadow: 0 0 0 1px rgb(0 0 0 / 0.6);
		transform: translate(-50%, -50%);
		pointer-events: none;
	}

	.hint {
		margin: -4px 0 0;
	}

	.field-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 8px;
	}

	.frame:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 3px;
	}

	.frame img {
		position: absolute;
		max-width: none;
		pointer-events: none;
	}

	/* Clear parts of the finished badge show the checkerboard, not the
	   picture under it, so what the fill took out is plain to see. */
	.finished {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		border-radius: 50%;
		pointer-events: none;
		opacity: 0;
		background-color: #e8e8ee;
		background-image:
			linear-gradient(45deg, #c9c9d2 25%, transparent 25%, transparent 75%, #c9c9d2 75%),
			linear-gradient(45deg, #c9c9d2 25%, transparent 25%, transparent 75%, #c9c9d2 75%);
		background-size: 16px 16px;
		background-position:
			0 0,
			8px 8px;
	}

	.finished.shown {
		opacity: 1;
	}

	.hole {
		position: absolute;
		inset: 0;
		border-radius: 50%;
		box-shadow: 0 0 0 999px rgb(0 0 0 / 0.55);
		outline: 2px solid rgb(255 255 255 / 0.85);
		pointer-events: none;
	}

	.empty-frame,
	.pick {
		width: min(260px, 100%);
		aspect-ratio: 1;
		display: grid;
		place-content: center;
		justify-items: center;
		gap: 6px;
		border-radius: var(--radius);
		border: 1.5px dashed var(--line-strong);
		text-align: center;
	}

	.pick {
		cursor: pointer;
	}

	.pick:hover,
	.pick:focus-within {
		border-color: var(--accent);
	}

	.pick input,
	.file input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
	}

	.file {
		position: relative;
	}

	.zoom,
	.slider {
		display: grid;
		gap: 4px;
		width: 100%;
		font-size: 0.86rem;
	}

	.zoom {
		max-width: 260px;
	}

	.note {
		max-width: 260px;
		margin: 0;
		font-size: 0.82rem;
		color: var(--warn, var(--text-dim));
		text-align: center;
	}

	.field {
		display: grid;
		gap: 6px;
		font-size: 0.86rem;
	}

	.group {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding-top: 14px;
		border-top: 1px solid var(--line);
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.tools {
		align-self: flex-start;
		max-width: 100%;
	}

	.tools button {
		white-space: nowrap;
	}

	.seed {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		margin: 0;
	}

	.dot {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
	}

	.link {
		border: 0;
		background: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		cursor: pointer;
	}

	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.swatch {
		width: 30px;
		height: 30px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		cursor: pointer;
	}

	.swatch.none {
		background-image: linear-gradient(
			135deg,
			transparent 45%,
			var(--bad) 45%,
			var(--bad) 55%,
			transparent 55%
		) !important;
	}

	.swatch.on {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.shades {
		align-self: flex-start;
	}

	/* The badge as it will be seen, on a light page and a dark one. */
	.previews {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 10px;
	}

	.tone {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		padding: 14px 10px;
		border-radius: var(--radius-sm);
		min-height: 230px;
		justify-content: center;
	}

	.tone[data-tone='light'] {
		background: #f6f5f8;
		color: #16151c;
	}

	.tone[data-tone='dark'] {
		background: #08080c;
		color: #f2f1f6;
	}

	.as-name,
	.lines {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		max-width: 100%;
	}

	.as-name b {
		font-size: 1.15rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.as-card {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		border-radius: 12px;
		background: color-mix(in srgb, currentColor 7%, transparent);
	}

	.face {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		background: color-mix(in srgb, currentColor 25%, transparent);
	}

	.lines b {
		font-size: 0.85rem;
	}

	.as-open {
		width: 132px;
		height: 132px;
	}

	@media (max-width: 720px) {
		.editor {
			inset: 0;
			transform: none;
			width: 100%;
			max-height: none;
			height: 100dvh;
			border-radius: 0;
		}

		.body {
			grid-template-columns: minmax(0, 1fr);
			padding: 14px 16px;
		}

		.tools {
			align-self: stretch;
			overflow-x: auto;
		}
	}
</style>
