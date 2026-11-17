<script lang="ts">
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { listLine } from '#lib/events';
	import { failure, type Failure } from '#lib/failure';
	import { toasts } from '#lib/toast.svelte';
	import type { EventSummary } from '#lib/types';

	let events = $state<EventSummary[]>([]);
	let loading = $state(true);
	let failed = $state<Failure | null>(null);

	async function load() {
		failed = null;
		try {
			events = (await api.events()).events;
		} catch (e) {
			failed = failure(e, { title: 'Could not load events', hint: 'Try again in a minute.' });
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	const now = Date.now();
	const groups = $derived(
		[
			{
				name: 'Now',
				events: events.filter((e) => e.phase === 'live' || e.phase === 'premiered')
			},
			{ name: 'Scheduled', events: events.filter((e) => e.phase === 'scheduled') },
			{ name: 'Past', events: events.filter((e) => e.phase === 'past').reverse() }
		].filter((g) => g.events.length)
	);

	function chip(e: EventSummary): string {
		if (e.on === false) return 'Off';
		if (e.phase === 'past') return 'Past';
		if (e.phase === 'scheduled') return 'Scheduled';
		return e.hidden ? 'Hidden from Home' : 'On Home';
	}

	async function show(e: EventSummary) {
		try {
			await api.showEvent(e.slug);
			await load();
		} catch (err) {
			toasts.error(err);
		}
	}
</script>

<svelte:head>
	<title>Events · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<h1>Events</h1>
	</div>

	{#if loading}
		<Spinner label="Loading events" />
	{:else if failed}
		<LoadFailed {failed} onretry={load} />
	{:else if !events.length}
		<Empty art="rest" title="No events yet" hint="Countdowns to the big releases show up here.">
			<a class="btn" href="/calendar">Calendar</a>
		</Empty>
	{:else}
		{#each groups as g (g.name)}
			<section class="group">
				<h2>{g.name}</h2>
				<ul>
					{#each g.events as e (e.slug)}
						<li class:past={e.phase === 'past'}>
							<a class="row" href="/events/{e.slug}">
								<span class="thumb">
									{#if e.title.backdrop_path}
										<img src={img(e.title.backdrop_path, 'w300')} alt="" loading="lazy" />
									{:else if e.title.poster_path}
										<img src={img(e.title.poster_path, 'w185')} alt="" loading="lazy" />
									{/if}
								</span>
								<span class="words">
									<strong>{e.title.name}</strong>
									<span class="line">{listLine(e, now)}</span>
								</span>
								<span class="pill" class:on={chip(e) === 'On Home'}>{chip(e)}</span>
							</a>
							{#if e.hidden && e.phase !== 'past'}
								<button class="btn ghost" onclick={() => show(e)}>Show on Home</button>
							{/if}
						</li>
					{/each}
				</ul>
			</section>
		{/each}
	{/if}
</div>

<style>
	.group {
		margin-bottom: 32px;
	}

	h2 {
		margin-bottom: 12px;
		font-size: 1.05rem;
	}

	ul {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 0;
	}

	li {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
	}

	.row {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 10px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		color: inherit;
		text-decoration: none;
		transition: background var(--fast) var(--ease);
	}

	.row:hover {
		background: var(--bg-elev-2);
	}

	.thumb {
		flex: none;
		width: 128px;
		aspect-ratio: 16 / 9;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.thumb img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.past .thumb img {
		filter: saturate(0.4);
		opacity: 0.7;
	}

	.words {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.line {
		font-size: 0.86rem;
		color: var(--text-dim);
	}

	.pill {
		flex: none;
	}

	.pill.on {
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 40%, transparent);
	}

	@media (max-width: 560px) {
		.thumb {
			width: 84px;
		}

		li {
			flex-wrap: wrap;
		}

		.row .pill {
			display: none;
		}
	}
</style>
