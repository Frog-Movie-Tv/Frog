<script lang="ts">
	import { img } from '#lib/api';
	import { formatScore } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import type { Kind, Status } from '#lib/types';
	import { STATUS_LABEL } from '#lib/types';

	interface Props {
		kind: Kind;
		tmdbId: number;
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
	}

	let {
		kind,
		tmdbId,
		name,
		posterPath,
		year = null,
		status = null,
		rating = null,
		watched = 0,
		total = 0,
		morph = false,
		kindBadge = false,
		size = 'w342'
	}: Props = $props();

	const src = $derived(img(posterPath, size));
	const progress = $derived(total > 0 ? Math.min(100, (watched / total) * 100) : 0);
	const scored = $derived(rating === null ? null : formatScore(rating, settings.current.scoreFormat));
	const show = $derived(settings.current);
</script>

<a
	class="poster"
	href="/title/{kind}/{tmdbId}"
	title={name}
	style:view-transition-name={morph ? `poster-${kind}-${tmdbId}` : undefined}
>
	<div class="art">
		{#if src}
			<img {src} alt="" loading="lazy" decoding="async" />
		{:else}
			<div class="placeholder"><span>{name.slice(0, 2).toUpperCase()}</span></div>
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

	.placeholder {
		width: 100%;
		height: 100%;
		display: grid;
		place-items: center;
		background: linear-gradient(160deg, var(--bg-elev-2), var(--bg-elev));
		color: var(--text-faint);
		font-weight: 700;
		font-size: 1.4rem;
		letter-spacing: 0.05em;
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
		backdrop-filter: blur(8px);
		background: rgba(10, 10, 14, 0.66);
		color: #fff;
		border: 1px solid rgba(255, 255, 255, 0.14);
	}

	/* Dark enough under white text in every theme, still clearly its hue. */
	.badge[data-status] {
		background: color-mix(in srgb, var(--status) 72%, black);
		border-color: color-mix(in srgb, var(--status) 60%, white 10%);
	}

	.badge.kind {
		background: rgba(10, 10, 14, 0.72);
		font-weight: 650;
	}

	.badge.kind.right {
		left: auto;
		right: 7px;
	}

	.score {
		position: absolute;
		right: 7px;
		bottom: 7px;
		padding: 3px 8px;
		border-radius: var(--radius-pill);
		font-size: 0.72rem;
		font-weight: 700;
		background: rgba(10, 10, 14, 0.72);
		backdrop-filter: blur(8px);
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
