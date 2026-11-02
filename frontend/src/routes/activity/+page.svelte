<script lang="ts">
	import { api } from '#lib/api';
	import ActivityCard from '#lib/components/ActivityCard.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import LoadMore from '#lib/components/LoadMore.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { prettyDate } from '#lib/format';
	import { toasts } from '#lib/toast.svelte';
	import type { ActivityItem } from '#lib/types';

	let items = $state<ActivityItem[]>([]);
	let next = $state<string | null>(null);
	let loading = $state(true);
	let more = $state(false);

	$effect(() => {
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
	<title>Activity · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<h1>Activity</h1>
	</div>

	{#if loading}
		<Spinner label="Loading activity" />
	{:else if !items.length}
		<Empty
			art="rest"
			title="Nothing yet"
			hint="What others on this Frog watch shows here, once their profile is on."
		/>
	{:else}
		{#each days as group (group.day)}
			<section class="day">
				<h2>{prettyDate(group.items[0].at)}</h2>
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
		position: sticky;
		top: var(--header-h);
		z-index: 1;
		margin: 0 0 12px;
		padding: 8px 0;
		font-size: 0.95rem;
		color: var(--text-dim);
		background: var(--bg);
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
