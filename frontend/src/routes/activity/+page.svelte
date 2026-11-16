<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import ActivityCard from '#lib/components/ActivityCard.svelte';
	import DiaryView from '#lib/components/DiaryView.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import LoadMore from '#lib/components/LoadMore.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { prettyDate } from '#lib/format';
	import { toasts } from '#lib/toast.svelte';
	import type { ActivityItem } from '#lib/types';

	// What was watched, two ways: by everyone here, or your own diary. One
	// page, the view in the address so Back, a link and a reload keep it.
	const VIEWS = ['everyone', 'diary'] as const;
	type View = (typeof VIEWS)[number];
	const view = $derived.by((): View => {
		const asked = page.url.searchParams.get('view');
		return VIEWS.find((v) => v === asked) ?? 'everyone';
	});

	function show(next: View) {
		void goto(next === 'everyone' ? '/activity' : `/activity?view=${next}`, {
			replace: true,
			reset: false
		});
	}

	let items = $state<ActivityItem[]>([]);
	let next = $state<string | null>(null);
	let loading = $state(true);
	let more = $state(false);

	let started = false;
	$effect(() => {
		// Read once, the first time Everyone is shown.
		if (view !== 'everyone' || started) return;
		started = true;
		void api
			.allActivity()
			.then((r) => {
				items = r.items;
				next = r.next;
			})
			.catch((e) => toasts.error(e))
			.finally(() => (loading = false));
	});

	async function loadMore() {
		if (!next || more) return;
		more = true;
		try {
			const r = await api.allActivity(next);
			const seen = new Set(items.map(key));
			items = [...items, ...r.items.filter((i) => !seen.has(key(i)))];
			next = r.next;
		} catch (e) {
			toasts.error(e);
		} finally {
			more = false;
		}
	}

	const key = (i: ActivityItem) => `${i.who.username}:${i.title.id}:${i.at}`;

	/** The cards under the day they happened, in your own time zone. */
	const days = $derived.by(() => {
		const out: { day: string; items: ActivityItem[] }[] = [];
		for (const item of items) {
			const d = new Date(item.at);
			const day = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
			const last = out.at(-1);
			if (last?.day === day) last.items.push(item);
			else out.push({ day, items: [item] });
		}
		return out;
	});
</script>

<svelte:head>
	<title>{view === 'diary' ? 'Diary' : 'Activity'} · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<h1>Activity</h1>
		<div class="segmented" role="group" aria-label="View">
			<button
				class:on={view === 'everyone'}
				aria-pressed={view === 'everyone'}
				onclick={() => show('everyone')}>Everyone</button
			>
			<button
				class:on={view === 'diary'}
				aria-pressed={view === 'diary'}
				onclick={() => show('diary')}>Diary</button
			>
		</div>
	</div>

	{#if view === 'diary'}
		<DiaryView />
	{:else if loading}
		<Spinner label="Loading activity" />
	{:else if !items.length}
		<Empty
			art="rest"
			title="Nothing yet"
			hint="What you and everyone on this Frog watch shows here."
		/>
	{:else}
		{#each days as group (group.day)}
			<section class="day">
				<h2 class="sticky-head">{prettyDate(group.items[0].at)}</h2>
				<div class="cards">
					{#each group.items as item (key(item))}
						<ActivityCard {item} />
					{/each}
				</div>
			</section>
		{/each}
		<LoadMore more={!!next} loading={more} onmore={loadMore} />
	{/if}
</div>

<style>
	.day {
		margin-bottom: 28px;
		animation: day-in 360ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	@keyframes day-in {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
	}

	h2 {
		margin: 0 -15px 12px;
		padding: 8px 14px;
		font-size: 0.95rem;
		color: var(--text-dim);
	}

	/* A phone's margin is narrower: the strip steps out only part of it. */
	@media (max-width: 720px) {
		h2 {
			margin-inline: -8px;
			padding-inline: 8px;
		}
	}

	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 280px), 1fr));
		gap: 12px;
	}

	@media (prefers-reduced-motion: reduce) {
		.day {
			animation: none;
		}
	}
</style>
