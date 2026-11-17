<script lang="ts">
	import { onMount } from 'svelte';
	import { api, img } from '#lib/api';
	import { afterLine, label } from '#lib/events';
	import { choreographyFor } from '#lib/premieres';
	import { Stage, glide, type Cast } from '#lib/premieres/stage';
	import type { EventSummary } from '#lib/types';

	/*
	 * The moment an event opens, played once per account over the whole
	 * screen: the stage (`lib/premieres/stage.ts`) draws, the event's own
	 * choreography says when, and this component holds the pieces they move.
	 * At the handoff the logo glides onto the hero's and the overlay fades, so
	 * the page underneath is simply there.
	 */

	interface Props {
		event: EventSummary;
		/** Played for an administrator: nothing is marked seen. */
		preview?: boolean;
		/** The hero's own words, which the logo glides onto. */
		target?: HTMLElement | null;
		/** Over, played or not. */
		ondone: (played: boolean) => void;
	}

	let { event, preview = false, target = null, ondone }: Props = $props();

	/** Pictures that take longer than this mean no premiere: the hero is enough. */
	const PICTURE_WAIT_MS = 3000;
	const FADE_MS = 800;

	const t = $derived(event.title);
	const r = $derived(event.readiness);
	const accent = $derived(event.accent.accent);
	const backdrop = $derived(t.backdrop_path ? img(t.backdrop_path, 'w1280') : null);
	const logo = $derived(t.logo_path ? img(t.logo_path, 'w500') : null);
	const badge = $derived(r?.ready_by_zero ? event.ready_badge : null);

	/** The line, with its first number apart so it can be counted up. */
	const line = $derived.by(() => {
		if (!r) return null;
		const l = afterLine(r);
		const inStrong = /\d/.test(l.strong);
		const m = /^(\D*)(\d+)(.*)$/.exec(inStrong ? l.strong : l.rest);
		return { ...l, inStrong, pre: m?.[1] ?? '', n: Number(m?.[2] ?? 0), post: m?.[3] ?? '' };
	});

	let playing = $state(false);
	let still = $state(false);

	let overlay: HTMLElement;
	let scene: HTMLElement;
	let backdropEl: HTMLElement;
	let leak: HTMLElement;
	let canvas: HTMLCanvasElement;
	let clock: HTMLElement;
	let tilesEl: HTMLElement;
	let seconds: HTMLElement;
	let body: HTMLElement;
	let labelEl: HTMLElement;
	let logoEl: HTMLElement;
	let ghostsEl: HTMLElement;
	let lineEl: HTMLElement;
	let countEl = $state<HTMLElement | null>(null);
	let actions: HTMLElement;
	let badgeEl: HTMLElement | null = $state(null);
	let top: HTMLElement;
	let bottom: HTMLElement;

	let stage: Stage | null = null;
	let handoff = 0;
	let finished = false;

	function load(src: string | null): Promise<void> {
		if (!src) return Promise.resolve();
		return new Promise((resolve, reject) => {
			const i = new Image();
			i.onload = () => resolve();
			i.onerror = () => reject(new Error('picture'));
			i.src = src;
		});
	}

	function pictures(): Promise<boolean> {
		const all = Promise.all([load(backdrop), load(logo), load(badge?.picture ?? null)]).then(
			() => true,
			() => false
		);
		const late = new Promise<boolean>((r) => setTimeout(() => r(false), PICTURE_WAIT_MS));
		return Promise.race([all, late]);
	}

	function done(played: boolean) {
		if (finished) return;
		finished = true;
		stage?.end();
		ondone(played);
	}

	/** The handoff: the logo onto the hero's, the overlay away. */
	function handOver() {
		const onto = target?.querySelector<HTMLElement>('.logo, h1') ?? null;
		glide(logoEl, onto, FADE_MS / 1000);
		overlay
			.animate([{ opacity: 1 }, { opacity: 0 }], {
				duration: FADE_MS,
				easing: 'cubic-bezier(.4,0,.2,1)',
				fill: 'forwards'
			})
			.finished.then(
				() => done(true),
				() => done(true)
			);
	}

	/** Tap, click, Esc or Space: straight to the handoff. */
	function skip() {
		if (!stage || finished || stage.t >= handoff) return;
		stage.jump(handoff - 0.01);
		for (const a of overlay.getAnimations({ subtree: true })) {
			try {
				a.finish();
			} catch {
				// An animation with no end has nothing to finish.
			}
		}
	}

	function keys(e: KeyboardEvent) {
		if (e.key === 'Escape' || e.key === ' ' || e.key === 'Enter') {
			e.preventDefault();
			skip();
		}
	}

	onMount(() => {
		still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		let gone = false;
		const resize = () => stage?.size();
		window.addEventListener('resize', resize);
		window.addEventListener('keydown', keys);

		void (async () => {
			const [ready, first] = await Promise.all([
				pictures(),
				preview
					? Promise.resolve(true)
					: api.eventSeen(event.slug).then(
							(a) => a.first,
							() => false
						)
			]);
			if (gone) return;
			// Another device played it, or the pictures are not here: the hero is enough.
			if (!ready || !first) return done(false);
			playing = true;
			if (still) {
				// Moving pictures are switched off: a cross-fade, nothing more.
				overlay.style.opacity = '1';
				overlay
					.animate([{ opacity: 1 }, { opacity: 0 }], {
						duration: 600,
						delay: 300,
						fill: 'forwards'
					})
					.finished.then(
						() => done(true),
						() => done(true)
					);
				return;
			}
			const cast: Cast = {
				overlay,
				scene,
				backdrop: backdropEl,
				leak,
				clock,
				tiles: Array.from(tilesEl.children) as HTMLElement[],
				seconds,
				body,
				label: labelEl,
				logo: logoEl,
				ghosts: Array.from(ghostsEl.children) as HTMLElement[],
				line: lineEl,
				count: countEl ?? lineEl,
				actions,
				badge: badgeEl,
				bars: [top, bottom]
			};
			stage = new Stage(cast, canvas, accent);
			const play = choreographyFor(event.slug);
			handoff = play(stage, cast, {
				accent,
				ready: !!badge,
				essentials: r?.essentials ?? 0
			}).handoff;
			stage.at(handoff, handOver);
			// Kept where a frame-by-frame check can read it.
			const watched = stage;
			Object.assign(window, {
				frogPremiere: {
					frames: watched.frames,
					slug: event.slug,
					handoff,
					get t() {
						return watched.t;
					}
				}
			});
			stage.start();
		})();

		return () => {
			gone = true;
			window.removeEventListener('resize', resize);
			window.removeEventListener('keydown', keys);
			stage?.end();
		};
	});
</script>

<div
	class="premiere"
	class:playing
	bind:this={overlay}
	style:--event-accent={accent}
	role="presentation"
	onclick={skip}
>
	<div class="scene" bind:this={scene}>
		<div class="backdrop" bind:this={backdropEl}>
			{#if backdrop}<img src={backdrop} alt="" />{/if}
		</div>
		<div class="leak" bind:this={leak}></div>
		<div class="vignette"></div>
		<canvas bind:this={canvas}></canvas>
	</div>

	<div class="clock" bind:this={clock} aria-hidden="true">
		<div class="tiles" bind:this={tilesEl}>
			<span class="tile"><b>0</b><i>days</i></span>
			<span class="tile"><b>00</b><i>hours</i></span>
			<span class="tile"><b>00</b><i>min</i></span>
			<span class="tile"><b bind:this={seconds}>04</b><i>sec</i></span>
		</div>
	</div>

	<div class="body" bind:this={body}>
		<span class="label" bind:this={labelEl}>{label(event, true)}</span>
		<div class="mark">
			<div class="ghosts" bind:this={ghostsEl} aria-hidden="true">
				{#each [0, 1, 2] as g (g)}
					{#if logo}<img class="ghost" src={logo} alt="" />{:else}<span class="ghost name"
							>{t.name}</span
						>{/if}
				{/each}
			</div>
			<div class="logo" bind:this={logoEl}>
				{#if logo}<img src={logo} alt={t.name} />{:else}<span class="name">{t.name}</span>{/if}
			</div>
		</div>
		<div class="said">
			<p class="line" bind:this={lineEl}>
				{#if line}
					{#if line.inStrong}
						<strong>{line.pre}<span bind:this={countEl} data-to={line.n}>0</span>{line.post}</strong
						>
						{line.rest}
					{:else}
						<strong>{line.strong}</strong>
						{line.pre}<span bind:this={countEl} data-to={line.n}>0</span>{line.post}
					{/if}
				{:else}
					<strong>Out now.</strong>
					<span bind:this={countEl} data-to="0" hidden>0</span>
				{/if}
			</p>
			{#if badge}
				<img class="badge" bind:this={badgeEl} src={badge.picture} alt={badge.name} />
			{/if}
		</div>
		<div class="actions" bind:this={actions} aria-hidden="true">
			{#if r && !r.ready && r.next}
				<span class="btn primary">Catch up · {r.next.name}</span>
			{:else if t.kind === 'movie'}
				<span class="btn primary">I've seen it</span>
			{/if}
		</div>
	</div>

	<div class="bar top" bind:this={top}></div>
	<div class="bar bottom" bind:this={bottom}></div>
	{#if playing && !still}<span class="skip">Tap to skip</span>{/if}
</div>

<style>
	.premiere {
		position: fixed;
		inset: 0;
		z-index: 1000;
		overflow: hidden;
		background: #000;
		color: #fff;
		opacity: 0;
		cursor: pointer;
		overflow-wrap: normal;
		user-select: none;
		-webkit-tap-highlight-color: transparent;
	}

	/* Waiting on its pictures, it is there but not in the way. */
	.premiere:not(.playing) {
		pointer-events: none;
	}

	.scene {
		position: absolute;
		inset: 0;
		will-change: transform;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		opacity: 0;
		will-change: transform, opacity;
	}

	.backdrop img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 20%;
	}

	.leak {
		position: absolute;
		inset: -10% -30%;
		opacity: 0;
		background: linear-gradient(
			100deg,
			transparent 30%,
			color-mix(in srgb, var(--event-accent) 55%, #fff) 50%,
			transparent 70%
		);
		mix-blend-mode: screen;
		pointer-events: none;
	}

	.vignette {
		position: absolute;
		inset: 0;
		background:
			radial-gradient(ellipse at center, transparent 40%, rgb(0 0 0 / 0.75) 100%),
			linear-gradient(to top, rgb(0 0 0 / 0.7), transparent 45%);
		pointer-events: none;
	}

	canvas {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
	}

	.clock {
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
	}

	.tiles {
		display: flex;
		gap: 12px;
	}

	.tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		min-width: 92px;
		padding: 16px 12px 12px;
		border: 1px solid rgb(255 255 255 / 0.18);
		border-radius: 14px;
		background: rgb(255 255 255 / 0.06);
	}

	.tile b {
		font-size: 3rem;
		font-weight: 800;
		line-height: 1;
		font-variant-numeric: tabular-nums;
		letter-spacing: -0.03em;
		transition:
			color 250ms,
			text-shadow 250ms;
	}

	.tile i {
		margin-top: 8px;
		font-style: normal;
		font-size: 0.7rem;
		font-weight: 700;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		opacity: 0.6;
	}

	/* Zero: the digits burn. */
	.clock:global(.hot) b {
		color: #fff;
		text-shadow:
			0 0 12px var(--event-accent),
			0 0 34px var(--event-accent),
			0 0 70px var(--event-accent);
	}

	.body {
		position: absolute;
		left: 50%;
		top: 54%;
		transform: translate(-50%, -50%);
		width: min(760px, calc(100vw - 32px));
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 18px;
		text-align: center;
	}

	.label {
		opacity: 0;
		font-size: 0.85rem;
		font-weight: 800;
		text-transform: uppercase;
		color: var(--event-accent);
		text-shadow: 0 1px 12px rgb(0 0 0 / 0.7);
	}

	.mark {
		position: relative;
		display: grid;
	}

	.ghosts {
		display: contents;
	}

	.ghost,
	.logo {
		grid-area: 1 / 1;
		justify-self: center;
		opacity: 0;
	}

	.logo {
		position: relative;
		overflow: hidden;
	}

	.ghost,
	.logo img {
		display: block;
		max-width: min(620px, 84vw);
		max-height: 200px;
		object-fit: contain;
		filter: drop-shadow(0 6px 24px rgb(0 0 0 / 0.7));
	}

	.ghost {
		filter: blur(3px);
	}

	.name {
		font-size: clamp(2.6rem, 8vw, 5.6rem);
		font-weight: 800;
		letter-spacing: -0.045em;
		line-height: 1;
	}

	/* A light that crosses the logo once. */
	.logo::after {
		content: '';
		position: absolute;
		inset: 0;
		background: linear-gradient(
			105deg,
			transparent 35%,
			rgb(255 255 255 / 0.75) 50%,
			transparent 65%
		);
		transform: translateX(-120%);
		mix-blend-mode: overlay;
		pointer-events: none;
	}

	.logo:global(.sweep)::after {
		animation: sweep 900ms cubic-bezier(0.4, 0, 0.2, 1) forwards;
	}

	.said {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: center;
		gap: 14px;
		perspective: 600px;
	}

	.line {
		opacity: 0;
		font-size: 1.15rem;
		text-shadow: 0 1px 10px rgb(0 0 0 / 0.6);
	}

	.line strong {
		font-weight: 800;
		margin-right: 4px;
	}

	.line span {
		font-variant-numeric: tabular-nums;
	}

	.badge {
		width: 64px;
		height: 64px;
		opacity: 0;
		filter: drop-shadow(0 0 18px color-mix(in srgb, var(--event-accent) 70%, transparent));
	}

	.actions {
		opacity: 0;
		display: flex;
		gap: 10px;
	}

	.actions .btn.primary {
		min-height: 48px;
		padding: 0 26px;
		background: #fff;
		color: #000;
	}

	.bar {
		position: absolute;
		left: 0;
		right: 0;
		/* 2.39:1 inside whatever the screen is. */
		height: max(0px, calc((100vh - 100vw / 2.39) / 2));
		min-height: 6vh;
		background: #000;
	}

	/* A phone held upright is no cinema screen: true 2.39:1 would leave a
	   strip across its middle, so the bars are only a frame. */
	@media (orientation: portrait) {
		.bar {
			height: 9vh;
			min-height: 0;
		}
	}

	.bar.top {
		top: 0;
		transform: translateY(-100%);
	}

	.bar.bottom {
		bottom: 0;
		transform: translateY(100%);
	}

	.skip {
		position: absolute;
		right: 18px;
		bottom: 14px;
		z-index: 2;
		font-size: 0.74rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		opacity: 0.45;
	}

	@keyframes sweep {
		to {
			transform: translateX(120%);
		}
	}

	@media (max-width: 560px) {
		.tiles {
			gap: 6px;
		}

		.tile {
			min-width: 64px;
			padding: 12px 6px 10px;
		}

		.tile b {
			font-size: 2.1rem;
		}

		.logo img,
		.ghost {
			max-height: 120px;
		}

		.line {
			font-size: 1rem;
		}

		.badge {
			width: 52px;
			height: 52px;
		}
	}

	/* Without motion it is only the picture, fading into the page. */
	@media (prefers-reduced-motion: reduce) {
		.backdrop,
		.label,
		.logo,
		.line,
		.badge {
			opacity: 1;
		}

		.clock,
		.ghosts,
		.actions {
			display: none;
		}
	}
</style>
