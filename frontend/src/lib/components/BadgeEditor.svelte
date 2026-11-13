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
	let tool = $state<'move' | 'pick' | 'spot'>('move');
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
				const { canvas, seed } = await render(src, s, PREVIEW, filler);
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

	function down(event: PointerEvent) {
		if (!settings) return;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
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
					class:tapping={tool !== 'move' && settings.removal.on}
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
					onwheel={wheel}
					onkeydown={key}
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
						width={PREVIEW}
						height={PREVIEW}
						aria-hidden="true"
					></canvas>
					<span class="hole" aria-hidden="true"></span>
				</div>
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
						<input type="checkbox" class="toggle-switch" bind:checked={settings.removal.on} />
					</label>
					{#if settings.removal.on}
						<label class="slider">
							<span>Tolerance <em class="faint">{settings.removal.tolerance}</em></span>
							<input
								type="range"
								min="0"
								max="60"
								step="1"
								bind:value={settings.removal.tolerance}
							/>
						</label>
						<label class="slider">
							<span>Edge softness <em class="faint">{settings.removal.softness}</em></span>
							<input
								type="range"
								min="0"
								max="40"
								step="1"
								bind:value={settings.removal.softness}
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
						<p class="faint small seed">
							<span class="dot" style:background={hex(settings.removal.seed ?? found)}></span>
							{settings.removal.seed ? 'Background picked' : 'Background from the edge'}
							{#if settings.removal.spots.length}
								· {settings.removal.spots.length}
								{settings.removal.spots.length === 1 ? 'spot' : 'spots'} cleared
							{/if}
							{#if settings.removal.seed || settings.removal.spots.length}
								<button
									class="link"
									onclick={() => {
										if (!settings) return;
										settings.removal.seed = null;
										settings.removal.spots = [];
									}}>Start over</button
								>
							{/if}
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
