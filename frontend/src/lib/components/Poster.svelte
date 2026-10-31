<script lang="ts">
	import { img } from '#lib/api';
	import Icon from './Icon.svelte';
	import { formatScore } from '#lib/format';
	import { morph as opened } from '#lib/morph.svelte';
	import { settings } from '#lib/settings.svelte';
	import { titleHref, type TitleRef } from '#lib/titles';
	import type { HTMLAnchorAttributes } from 'svelte/elements';
	import type { Kind, Status } from '#lib/types';
	import { STATUS_LABEL } from '#lib/types';

	interface Props extends HTMLAnchorAttributes {
		kind: Kind;
		/** Which title: by Frog's id, or as a provider names it. */
		ref: TitleRef;
		name: string;
		posterPath: string | null;
		year?: string | number | null;
		status?: Status | null;
		rating?: number | null;
		watched?: number;
		total?: number;
		/** Enables the shared-element morph into the title page. */
		morph?: boolean;
		/** Show a Film / Show badge, for mixed result lists. */
		kindBadge?: boolean;
		size?: string;
		/** A backdrop to crop when TMDB has no poster, as small titles often do. */
		fallbackPath?: string | null;
		/** Library's edit mode: ticked or not. Unset, the card has no tick. */
		selected?: boolean;
	}

	let {
		kind,
		ref,
		name,
		posterPath,
		year = null,
		status = null,
		rating = null,
		watched = 0,
		total = 0,
		morph = false,
		kindBadge = false,
		size = 'w342',
		fallbackPath = null,
		selected,
		...rest
	}: Props = $props();

	const src = $derived(img(posterPath, size));
	const fallback = $derived(posterPath ? null : img(fallbackPath, 'w780'));
	const progress = $derived(total > 0 ? Math.min(100, (watched / total) * 100) : 0);
	const scored = $derived(
		rating === null ? null : formatScore(rating, settings.current.scoreFormat)
	);
	const show = $derived(settings.current);
	const href = $derived(titleHref({ ...ref, kind }));
	// The title page names its poster by Frog's id; a provider's address is
	// only a step on the way there.
	const key = $derived(ref.id ? `t${ref.id}` : `${kind}-${ref.tmdb_id ?? ref.anilist_id}`);
	const named = $derived(morph && opened.key === key);
</script>

<a
	class="poster"
	href={href ?? undefined}
	title={name}
	style:view-transition-name={named ? `poster-${key}` : undefined}
	onclick={(e) => {
		if (!morph) return;
		opened.key = key;
		// Named now, before the transition takes its picture of this page.
		e.currentTarget.style.viewTransitionName = `poster-${key}`;
	}}
	{...rest}
>
	<div class="art" class:picked={selected}>
		{#if src}
			<img {src} alt="" loading="lazy" decoding="async" />
		{:else}
			{#if fallback}
				<img class="cropped" src={fallback} alt="" loading="lazy" decoding="async" />
			{:else}
				<!-- No artwork anywhere: a drawn card rather than an empty box. -->
				<div class="placeholder">
					<Icon name={kind === 'tv' ? 'tv' : 'film'} size={26} />
					<span>{name}</span>
				</div>
			{/if}
		{/if}

		{#if status && show.showBadge}
			<span class="badge" data-status={status}>{STATUS_LABEL[status]}</span>
		{:else if kindBadge}
			<span class="badge kind">{kind === 'tv' ? 'Show' : 'Film'}</span>
		{/if}

		{#if kindBadge && status && show.showBadge}
			<span class="badge kind right">{kind === 'tv' ? 'Show' : 'Film'}</span>
		{/if}

		{#if scored && show.showRating}
			<span class="score">{scored}</span>
		{/if}

		{#if progress > 0 && progress < 100 && show.showProgress}
			<div class="progress"><i style:width="{progress}%"></i></div>
		{/if}

		{#if selected !== undefined}
			<span class="tick" class:on={selected} aria-hidden="true">
				{#if selected}<Icon name="check" size={15} />{/if}
			</span>
		{/if}
	</div>

	<div class="meta">
		<span class="name">{name}</span>
		{#if year && show.showYear}<span class="year">{year}</span>{/if}
	</div>
</a>

<style>
	.poster {
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-width: 0;
	}

	.art {
		position: relative;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		box-shadow: var(--shadow-1);
		transition:
			transform var(--slow) var(--ease),
			box-shadow var(--slow) var(--ease);
	}

	.poster:hover .art,
	.poster:focus-visible .art {
		transform: translateY(-4px) scale(1.015);
		box-shadow: var(--shadow-2);
	}

	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	/* A landscape backdrop standing in for a poster: cropped to the middle,
	   where the subject usually is. */
	.cropped {
		object-position: center;
	}

	.placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 14px;
		text-align: center;
		background:
			radial-gradient(
				120% 70% at 50% 0%,
				color-mix(in srgb, var(--accent) 16%, transparent),
				transparent 70%
			),
			linear-gradient(160deg, var(--bg-elev-2), var(--bg-elev));
		color: var(--text-faint);
	}

	.placeholder span {
		font-family: var(--font-display);
		font-size: 0.86rem;
		font-weight: 650;
		line-height: 1.25;
		color: var(--text-dim);
		display: -webkit-box;
		-webkit-line-clamp: 4;
		line-clamp: 4;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.badge {
		position: absolute;
		top: 7px;
		left: 7px;
		padding: 3px 8px;
		border-radius: var(--radius-pill);
		font-size: 0.64rem;
		font-weight: 700;
		letter-spacing: 0.03em;
		text-transform: uppercase;
		/* No backdrop blur: on a library of hundreds it halved the frame rate
		   while scrolling. A darker fill keeps the text as legible. */
		background: rgba(10, 10, 14, 0.78);
		color: #fff;
		border: 1px solid rgba(255, 255, 255, 0.14);
	}

	/* Dark enough under white text in every theme, still clearly its hue. */
	.badge[data-status] {
		background: color-mix(in srgb, var(--status) 72%, black);
		border-color: color-mix(in srgb, var(--status) 60%, white 10%);
	}

	.badge.kind {
		background: rgba(10, 10, 14, 0.8);
		font-weight: 650;
	}

	.badge.kind.right {
		left: auto;
		right: 7px;
	}

	/* Library's edit mode. Bottom left: the badges keep the top, the score
	   the bottom right. */
	.tick {
		position: absolute;
		left: 7px;
		bottom: 7px;
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: 50%;
		border: 2px solid #fff;
		background: rgba(10, 10, 14, 0.55);
		color: var(--accent-ink);
	}

	.tick.on {
		border-color: var(--accent);
		background: var(--accent);
	}

	.art.picked {
		outline: 3px solid var(--accent);
		outline-offset: 1px;
	}

	.score {
		position: absolute;
		right: 7px;
		bottom: 7px;
		padding: 3px 8px;
		border-radius: var(--radius-pill);
		font-size: 0.72rem;
		font-weight: 700;
		background: rgba(10, 10, 14, 0.8);
		border: 1px solid rgba(255, 255, 255, 0.14);
		color: #fff;
	}

	.progress {
		position: absolute;
		inset: auto 0 0 0;
		height: 3px;
		background: rgba(255, 255, 255, 0.18);
	}

	.progress i {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.meta {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	.name {
		font-size: 0.82rem;
		font-weight: 550;
		line-height: 1.3;
		overflow: hidden;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
	}

	.year {
		font-size: 0.72rem;
		color: var(--text-faint);
	}
</style>
