<script lang="ts">
	import { goto } from '$app/navigation';
	import { addAs, seenIt } from '#lib/add';
	import { api, img } from '#lib/api';
	import { afterLine, isOut, label, readyLine } from '#lib/events';
	import { year, runtime } from '#lib/format';
	import { showFrom } from '#lib/menu.svelte';
	import { titleHref } from '#lib/titles';
	import { toasts } from '#lib/toast.svelte';
	import type { EventSummary } from '#lib/types';
	import Countdown from './Countdown.svelte';
	import Icon from './Icon.svelte';

	interface Props {
		events: EventSummary[];
		/** Hidden from here: Home drops it at once. */
		onhide: (slug: string) => void;
		/** Something changed that Home should read again. */
		onchanged: () => void;
		/** The countdown reached zero with the page open. */
		onzero?: (e: EventSummary) => void;
		/** The words' own block, which the premiere glides onto. */
		body?: HTMLElement | null;
		/** Held back while the premiere plays over it. */
		waiting?: boolean;
	}

	let {
		events,
		onhide,
		onchanged,
		onzero,
		body = $bindable(null),
		waiting = false
	}: Props = $props();

	let index = $state(0);
	const at = $derived(Math.min(index, Math.max(0, events.length - 1)));
	const e = $derived(events[at] as EventSummary | undefined);
	const t = $derived(e?.title ?? null);

	// Out or not is the browser's clock, so the hero turns at zero without
	// asking the server again.
	let now = $state(Date.now());
	$effect(() => {
		const every = setInterval(() => (now = Date.now()), 15_000);
		return () => clearInterval(every);
	});
	const out = $derived(!!e && isOut(e, now));
	const r = $derived(e?.readiness ?? null);
	const line = $derived(r ? (out ? afterLine(r) : readyLine(r)) : null);
	const share = $derived(r && r.essentials ? r.essentials_seen / r.essentials : 0);
	const accent = $derived(e?.accent.accent ?? 'var(--accent)');

	const backdrop = $derived(t?.backdrop_path ?? null);
	const srcset = $derived(
		backdrop
			? `${img(backdrop, 'w780')} 780w, ${img(backdrop, 'w1280')} 1280w, ${img(backdrop, 'original')} 1920w`
			: undefined
	);

	// Two events on one day take turns, as the usual hero's slides do.
	const SLIDE_MS = 10_000;
	let hovered = $state(false);
	$effect(() => {
		if (events.length < 2 || hovered) return;
		const turn = setTimeout(() => (index = (at + 1) % events.length), SLIDE_MS);
		return () => clearTimeout(turn);
	});

	let busy = $state(false);

	async function watchlist() {
		if (!t) return;
		busy = true;
		try {
			await addAs({ id: t.id, name: t.name, kind: t.kind }, 'planned', onchanged);
		} finally {
			busy = false;
		}
	}

	async function seen() {
		if (!t) return;
		busy = true;
		try {
			await seenIt({ id: t.id, name: t.name, kind: t.kind }, !!t.entry, onchanged);
		} finally {
			busy = false;
		}
	}

	async function hide(ev: EventSummary) {
		try {
			await api.hideEvent(ev.slug);
			onhide(ev.slug);
			toasts.undoable('Hidden from Home. It stays on Events, without reminders.', async () => {
				await api.showEvent(ev.slug);
				onchanged();
			});
		} catch (err) {
			toasts.error(err);
		}
	}

	function more(ev: MouseEvent) {
		const here = e;
		if (!here) return;
		showFrom(
			ev.currentTarget as HTMLElement,
			{
				title: here.title.name,
				items: () => [
					{ label: 'Event page', icon: 'calendar', run: () => void goto(`/events/${here.slug}`) },
					{ label: 'Hide from Home', icon: 'eye-off', run: () => hide(here) }
				]
			},
			ev
		);
	}

	/** Seen already, by the film's own record. */
	const watched = $derived(!!t && t.kind === 'movie' && t.entry?.status === 'completed');
</script>

{#if e && t}
	<header
		class="hero"
		class:waiting
		style:--event-accent={accent}
		role="region"
		aria-label={events.length > 1 ? 'Events' : `Countdown to ${t.name}`}
		onpointerenter={(ev) => {
			if (ev.pointerType === 'mouse') hovered = true;
		}}
		onpointerleave={() => (hovered = false)}
	>
		{#key e.slug}
			<div class="backdrop">
				{#if backdrop}
					<img src={img(backdrop, 'w1280')} {srcset} sizes="100vw" alt="" fetchpriority="high" />
				{/if}
			</div>
		{/key}
		<div class="veil" aria-hidden="true"></div>

		<div class="stage">
			{#key e.slug}
				<div class="hero-body" bind:this={body}>
					<span class="hero-label" class:out>{label(e, out)}</span>

					<a class="name-link" href={titleHref(t)}>
						{#if t.logo_path}
							<img class="logo" src={img(t.logo_path, 'w500')} alt={t.name} />
						{:else}
							<h1>{t.name}</h1>
						{/if}
					</a>

					{#if !out}
						<Countdown zero={e.zero} onzero={() => onzero?.(e)} />
					{:else}
						<p class="facts">
							{#if year(t.release_date)}<span>{year(t.release_date)}</span>{/if}
							{#if t.kind === 'movie' && t.runtime}<span>{runtime(t.runtime)}</span>{/if}
							{#if t.genres.length}<span>{t.genres.slice(0, 3).join(' · ')}</span>{/if}
						</p>
					{/if}

					{#if r && line}
						<p class="ready">
							<svg class="ring" viewBox="0 0 36 36" aria-hidden="true">
								<circle cx="18" cy="18" r="15" class="track" />
								<circle
									cx="18"
									cy="18"
									r="15"
									class="done"
									pathLength="100"
									stroke-dasharray="{Math.round(share * 100)} 100"
								/>
							</svg>
							<span><strong>{line.strong}</strong> {line.rest}</span>
						</p>
					{/if}

					<div class="hero-actions">
						{#if out}
							{#if t.kind === 'movie' && !watched}
								<button class="btn primary" disabled={busy} onclick={seen}>
									<Icon name="check" size={16} />
									I've seen it
								</button>
							{:else}
								<a class="btn primary" href={titleHref(t)}>
									<Icon name="play" size={14} filled />
									{t.kind === 'movie' ? 'View film' : 'Watch now'}
								</a>
							{/if}
							{#if r && !r.ready && r.next}
								<a class="btn on-picture" href="/t/{r.next.title_id}">
									Catch up · {r.next.name}{r.next.code ? ` ${r.next.code}` : ''}
								</a>
							{/if}
						{:else if r && r.next}
							<a class="btn primary" href="/t/{r.next.title_id}">
								<Icon name="play" size={14} filled />
								Catch up · {r.next.name}{r.next.code ? ` ${r.next.code}` : ''}
							</a>
						{:else if !e.catch_up && !t.entry}
							<button class="btn primary" disabled={busy} onclick={watchlist}>
								<Icon name="plus" size={16} />
								Add to watchlist
							</button>
						{/if}
						<a class="btn on-picture" href="/events/{e.slug}">Event page</a>
						<button
							class="btn on-picture more"
							aria-label="More"
							aria-haspopup="menu"
							onclick={more}
						>
							<Icon name="more" size={18} />
						</button>
					</div>
				</div>
			{/key}
		</div>

		{#if events.length > 1}
			<div class="dots" role="group" aria-label="Choose an event">
				{#each events as ev, i (ev.slug)}
					<button
						class="dot"
						class:on={i === at}
						aria-label="{i + 1} of {events.length}: {ev.title.name}"
						aria-current={i === at ? 'true' : undefined}
						onclick={() => (index = i)}
					>
						<span class="mark"></span>
					</button>
				{/each}
			</div>
		{/if}
	</header>
{/if}

<style>
	.hero {
		position: relative;
		isolation: isolate;
		min-height: 88vh;
		display: flex;
		align-items: flex-end;
		padding: calc(var(--header-h) + 48px) var(--page-pad) 72px;
		overflow: hidden;
	}

	/* The premiere draws this hero itself, then hands over to it. */
	.hero.waiting .hero-body {
		visibility: hidden;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		z-index: -2;
		overflow: hidden;
		mask-image: linear-gradient(to top, transparent 0, #000 34%);
		animation: picture-in 900ms cubic-bezier(0.4, 0, 0.2, 1) backwards;
	}

	.backdrop img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 20%;
	}

	.veil {
		position: absolute;
		inset: 0;
		z-index: -1;
		background:
			linear-gradient(to right, color-mix(in srgb, var(--bg) 86%, transparent), transparent 66%),
			linear-gradient(
				to top,
				color-mix(in srgb, var(--bg) 72%, transparent) 0,
				color-mix(in srgb, var(--bg) 40%, transparent) 34%,
				transparent 58%
			);
		mask-image: linear-gradient(to top, transparent 0, #000 28%);
	}

	.stage {
		display: grid;
		max-width: 1440px;
		margin: 0 auto;
		width: 100%;
	}

	.hero-body {
		grid-area: 1 / 1;
		align-self: end;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 14px;
		min-width: 0;
	}

	.hero-body > :global(*) {
		animation: words-in 620ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}
	.hero-body > :global(:nth-child(2)) {
		animation-delay: 60ms;
	}
	.hero-body > :global(:nth-child(3)) {
		animation-delay: 120ms;
	}
	.hero-body > :global(:nth-child(n + 4)) {
		animation-delay: 180ms;
	}

	.hero-label {
		font-size: 0.82rem;
		font-weight: 800;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--event-accent);
		text-shadow: 0 1px 10px rgb(0 0 0 / 0.5);
	}

	.name-link {
		color: inherit;
		text-decoration: none;
		max-width: 100%;
	}

	.logo {
		display: block;
		max-width: min(620px, 82vw);
		max-height: 200px;
		object-fit: contain;
		object-position: left bottom;
		filter: drop-shadow(0 4px 18px rgba(0, 0, 0, 0.65));
	}

	:global(:root[data-tone='light']) .logo {
		filter: drop-shadow(0 0 1px rgb(0 0 0 / 0.6)) drop-shadow(0 1px 3px rgb(0 0 0 / 0.45))
			drop-shadow(0 6px 22px rgb(0 0 0 / 0.55));
	}

	.hero h1 {
		font-size: clamp(3rem, 7vw, 5.6rem);
		letter-spacing: -0.045em;
	}

	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 14px;
		font-size: 0.9rem;
		color: var(--text);
	}

	.ready {
		display: flex;
		align-items: center;
		gap: 12px;
		max-width: 60ch;
		font-size: 0.98rem;
		color: var(--text);
		text-shadow: 0 1px 8px rgb(0 0 0 / 0.35);
	}

	.ready strong {
		font-weight: 800;
		margin-right: 4px;
	}

	.ring {
		flex: none;
		width: 34px;
		height: 34px;
		transform: rotate(-90deg);
	}

	.ring circle {
		fill: none;
		stroke-width: 3.4;
	}

	.ring .track {
		stroke: color-mix(in srgb, var(--text) 20%, transparent);
	}

	.ring .done {
		stroke: var(--event-accent);
		stroke-linecap: round;
		transition: stroke-dasharray 600ms var(--ease);
	}

	.hero-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		margin-top: 4px;
	}

	.hero-actions .btn.primary {
		min-height: 50px;
		padding: 0 28px;
		background: var(--text);
		color: var(--bg);
		font-size: 1rem;
		box-shadow: 0 10px 30px -10px rgba(0, 0, 0, 0.6);
	}

	.hero-actions .btn.on-picture {
		min-height: 50px;
		padding: 0 22px;
		background: color-mix(in srgb, var(--bg) 46%, transparent);
		backdrop-filter: var(--frost);
		border-color: var(--line-strong);
		color: var(--text);
		font-size: 1rem;
	}

	.hero-actions .more {
		padding: 0;
		width: 50px;
		justify-content: center;
	}

	.dots {
		position: absolute;
		z-index: 2;
		bottom: 24px;
		right: max(var(--page-pad), calc((100% - 1440px) / 2 + var(--page-pad)));
		display: flex;
	}

	.dot {
		display: grid;
		place-items: center;
		height: 28px;
		padding: 0 4px;
		border: 0;
		background: none;
		cursor: pointer;
	}

	.mark {
		display: block;
		width: 8px;
		height: 8px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--text) 36%, transparent);
		transition: width 300ms var(--ease);
	}

	.dot.on .mark {
		width: 32px;
		background: var(--text);
	}

	@keyframes words-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}

	@keyframes picture-in {
		from {
			opacity: 0;
		}
	}

	@media (max-width: 720px) {
		.hero {
			min-height: 76vh;
			padding-top: calc(var(--header-h) + 24px);
			padding-bottom: 52px;
		}

		.logo {
			max-height: 120px;
		}

		.dots {
			left: 0;
			right: 0;
			bottom: 14px;
			justify-content: center;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.backdrop,
		.hero-body > :global(*) {
			animation: none;
		}
	}
</style>
