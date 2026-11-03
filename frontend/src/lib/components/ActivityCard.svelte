<script lang="ts">
	import { activityLine, alsoLine } from '#lib/activity';
	import { img } from '#lib/api';
	import { relativeDate } from '#lib/format';
	import { titleHref } from '#lib/titles';
	import Avatar from './Avatar.svelte';
	import type { ActivityItem } from '#lib/types';

	interface Props {
		item: ActivityItem;
	}

	let { item }: Props = $props();

	const name = $derived(item.who.display_name || item.who.username);
	const poster = $derived(img(item.title.poster_path, 'w185'));
</script>

<a class="card" href={titleHref(item.title)}>
	<span class="poster" aria-hidden="true">
		{#if poster}<img src={poster} alt="" loading="lazy" decoding="async" />{/if}
	</span>
	<span class="words">
		<span class="who">
			<Avatar src={item.who.avatar} name={item.who.username} size={22} />
			<strong>{name}</strong>
		</span>
		<span class="line">{activityLine(item)}</span>
		<span class="faint when">
			{relativeDate(item.at)}
			{#if item.also}· {alsoLine(item.also)}{/if}
		</span>
	</span>
</a>

<style>
	.card {
		display: flex;
		gap: 12px;
		min-width: 0;
		height: 100%;
		padding: 10px;
		border-radius: var(--radius);
		background: var(--bg-elev);
		color: inherit;
		text-decoration: none;
		transition: background var(--fast) var(--ease);
	}

	.card:hover {
		background: var(--bg-elev-2);
	}

	.poster {
		flex: none;
		width: 62px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.poster img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.words {
		display: flex;
		flex-direction: column;
		gap: 5px;
		min-width: 0;
		flex: 1;
	}

	.who {
		display: flex;
		align-items: center;
		gap: 7px;
		min-width: 0;
		font-size: 0.82rem;
	}

	.who strong {
		min-width: 0;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.line {
		font-size: 0.88rem;
		line-height: 1.3;
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		overflow: hidden;
	}

	.when {
		margin-top: auto;
		font-size: 0.76rem;
	}
</style>
