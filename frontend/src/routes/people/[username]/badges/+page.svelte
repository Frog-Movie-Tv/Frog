<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { address } from '#lib/address';
	import { api } from '#lib/api';
	import {
		DIFFICULTY,
		GROUPS,
		LADDERS,
		SORTS,
		cards as toCards,
		conditionText,
		fromAddress,
		grouped,
		heldBy,
		highlights,
		narrow,
		share,
		sorted,
		toAddress,
		type BadgeCard,
		type Show,
		type Sort
	} from '#lib/badge-board';
	import { topSteps } from '#lib/badge-ladder';
	import BadgeDetail from '#lib/components/BadgeDetail.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import ProfileShell from '#lib/components/ProfileShell.svelte';
	import Select from '#lib/components/Select.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { prettyDate } from '#lib/format';
	import { moved, sortable } from '#lib/sortable';
	import { toasts } from '#lib/toast.svelte';
	import type { BadgeBoard, BadgeGroup, HeldBadge } from '#lib/types';
	import { uniqueBy } from '#lib/unique';

	const username = $derived(page.params.username ?? '');
	let board = $state<BadgeBoard | null>(null);
	let failed = $state(false);

	const first = fromAddress(address().searchParams);
	let show = $state<Show>(first.show);
	let sort = $state<Sort>(first.sort);
	let group = $state<BadgeGroup | 'all'>(first.group);
	let query = $state(first.query);
	let openId = $state<number | null>(null);

	async function load(name: string) {
		try {
			const b = await api.profileBadges(name);
			if (name !== username) return;
			board = {
				...b,
				earned: uniqueBy(b.earned, (x) => x.id),
				locked: uniqueBy(b.locked, (x) => x.id)
			};
		} catch {
			if (name === username) failed = true;
		}
	}

	$effect(() => {
		const name = username;
		board = null;
		failed = false;
		openId = null;
		void load(name);
	});

	// The choices live in the address, so Back comes back to them.
	$effect(() => {
		const wanted = toAddress({ show, sort, group, query });
		const timer = setTimeout(
			() => {
				if (wanted !== untrack(() => address().search)) {
					void goto(`${untrack(() => address().pathname)}${wanted}`, {
						shallow: true,
						replace: true,
						state: untrack(() => page.state)
					});
				}
			},
			query ? 300 : 0
		);
		return () => clearTimeout(timer);
	});

	const all = $derived(board ? toCards(board) : []);
	const accounts = $derived(board?.accounts ?? 0);
	const total = $derived(board?.total ?? 0);
	const earnedCount = $derived(all.filter((c) => c.earned).length);
	const percent = $derived(total ? Math.round((earnedCount / total) * 100) : 0);
	const shown = $derived(sorted(narrow(all, { show, group, query }), sort));
	const sections = $derived(
		sort === 'kind' ? grouped(shown) : [{ id: 'all', label: '', about: '', cards: shown }]
	);
	const best = $derived(highlights(all));
	const open = $derived(all.find((c) => c.id === openId) ?? null);
	const ladderOf = (c: BadgeCard) =>
		c.ladder
			? all.filter((x) => x.ladder === c.ladder).sort((a, b) => (a.step ?? 0) - (b.step ?? 0))
			: [];

	const groupOptions = [
		{ value: 'all', label: 'Every kind' },
		...GROUPS.map((g) => ({ value: g.id, label: g.label }))
	];
	const count = (n: number) => n.toLocaleString();
	const day = (d: string) => prettyDate(`${d}T12:00:00`);

	/* Your own: what you chose to wear beside your name, in your order, which
	   is the order of `board.earned`. Everyone sees every badge you hold here;
	   the choice is only what sits beside your name. */
	const held = $derived(board?.earned ?? []);
	const beside = $derived(topSteps(held.filter((b) => b.shown)));
	const tops = $derived(new Set(topSteps(held).map((b) => b.id)));

	async function arrange(order: HeldBadge[]) {
		if (!board) return;
		board.earned = order;
		try {
			await api.arrangeBadges(order.map((b) => ({ id: b.id, shown: b.shown })));
		} catch (e) {
			toasts.error(e);
			await load(username);
		}
	}

	/** Chosen goes to the front, so it shows beside your name at once. */
	function setPinned(id: number, value: boolean) {
		const it = held.find((b) => b.id === id);
		if (!it) return;
		if (value) void arrange([{ ...it, shown: true }, ...held.filter((b) => b.id !== id)]);
		else void arrange(held.map((b) => (b.id === id ? { ...b, shown: false } : b)));
	}

	// Follows what is chosen, and moves under a drag until it is saved.
	let besideOrder = $derived(beside);

	function saveBeside() {
		const ids = new Set(besideOrder.map((b) => b.id));
		void arrange([...besideOrder, ...held.filter((b) => !ids.has(b.id))]);
	}
</script>

<ProfileShell tab="badges">
	{#snippet children(profile)}
		{#if failed}
			<Empty art="rest" title="Badges could not be loaded" />
		{:else if board === null}
			<Spinner label="Loading badges" />
		{:else}
			<section class="summary card" aria-label="Badges earned">
				<div class="tally">
					<p>
						<strong>{count(earnedCount)}</strong>
						<span class="dim">of {count(total)} earned</span>
					</p>
					<div
						class="meter"
						role="progressbar"
						aria-label="Badges earned"
						aria-valuemin={0}
						aria-valuemax={total}
						aria-valuenow={earnedCount}
					>
						<span style:width="{percent}%"></span>
					</div>
					<span class="faint percent">{percent}% of everything there is</span>
				</div>
				{#if best.newest}
					{@const c = best.newest}
					<button class="pick" onclick={() => (openId = c.id)}>
						<img src={c.small} alt="" width="48" height="48" />
						<span>
							<span class="faint">Newest</span>
							<strong>{c.name}</strong>
							{#if c.day}<span class="faint">{day(c.day)}</span>{/if}
						</span>
					</button>
				{/if}
				{#if profile.me && best.next}
					{@const c = best.next}
					<button class="pick" onclick={() => (openId = c.id)}>
						<img class="grey" src={c.small} alt="" width="48" height="48" />
						<span>
							<span class="faint">Next up</span>
							<strong>{c.name}</strong>
							{#if c.progress}<span class="faint"
									>{count(c.progress.have)} of {count(c.progress.need)}</span
								>{/if}
						</span>
					</button>
				{/if}
			</section>

			{#if profile.me && held.length}
				<section class="beside card" aria-labelledby="beside-title">
					<div class="beside-text">
						<strong id="beside-title">Beside your name</strong>
						<span class="faint">
							{beside.length > 3
								? 'The first three show beside your name, the rest one tap away. Drag to change the order.'
								: beside.length
									? 'Drag to change the order. Open any badge you hold to add it.'
									: 'Open a badge you hold to wear it beside your name.'}
						</span>
					</div>
					<ol
						class="slots"
						use:sortable={{
							onmove: (from, to) => (besideOrder = moved(besideOrder, from, to)),
							onend: saveBeside
						}}
					>
						{#each besideOrder as b, i (b.id)}
							<li data-sort-item class:extra={i >= 3}>
								<button
									class="slot"
									data-sort-handle
									aria-label="{b.name}: drag or use the arrow keys to move it"
									title={b.name}
								>
									<img src={b.small} alt="" width="44" height="44" />
								</button>
								<button
									class="unpin"
									aria-label="Take {b.name} from beside your name"
									onclick={() => setPinned(b.id, false)}
								>
									<Icon name="x" size={12} />
								</button>
							</li>
						{/each}
						{#each Array(Math.max(0, 3 - besideOrder.length)) as _, i (i)}
							<li class="empty" aria-hidden="true"></li>
						{/each}
					</ol>
				</section>
			{/if}

			<div class="tools">
				<div class="searchbar">
					<Icon name="search" size={17} />
					<input
						bind:value={query}
						type="search"
						placeholder="Search badges"
						aria-label="Search badges"
						autocomplete="off"
						spellcheck="false"
						enterkeyhint="search"
					/>
				</div>
				<div class="segmented" role="group" aria-label="Show">
					<button class:on={show === 'all'} onclick={() => (show = 'all')}>All</button>
					<button class:on={show === 'earned'} onclick={() => (show = 'earned')}>Earned</button>
					<button class:on={show === 'locked'} onclick={() => (show = 'locked')}
						>Still to earn</button
					>
				</div>
				<Select
					label="Kind"
					value={group}
					options={groupOptions}
					onchange={(v) => (group = v as BadgeGroup | 'all')}
				/>
				<Select
					label="Sort"
					value={sort}
					options={SORTS.filter((s) => !s.own || profile.me)}
					onchange={(v) => (sort = v as Sort)}
				/>
			</div>

			{#if !shown.length}
				<p class="faint none">
					{query
						? `No badge matches “${query}”.`
						: show === 'earned'
							? profile.me
								? 'Your first one is not far off.'
								: 'None yet.'
							: 'Every badge earned.'}
				</p>
			{/if}

			{#each sections as section (section.id)}
				<section class="group" aria-label={section.label || 'Badges'}>
					{#if section.label}
						<header>
							<h2>{section.label} <span class="faint">{count(section.cards.length)}</span></h2>
							<p class="faint">{section.about}</p>
						</header>
					{/if}
					<ul class="grid">
						{#each section.cards as c, i (c.id)}
							<li style="--i: {Math.min(i, 18)}">
								<button class="badge-card" class:locked={!c.earned} onclick={() => (openId = c.id)}>
									<span class="top">
										<img src={c.small} alt="" width="64" height="64" loading="lazy" />
										<span class="names">
											{#if c.ladder && c.step !== null && c.steps}
												<span class="eyebrow"
													>{LADDERS[c.ladder] ?? c.ladder} · step {c.step + 1} of {c.steps}</span
												>
											{:else if c.condition}
												<span class="eyebrow">Made here · {conditionText(c.condition)}</span>
											{/if}
											<strong>{c.name}</strong>
											<span class="tags">
												{#if c.difficulty}
													<span class="tag difficulty d{c.difficulty}"
														>{DIFFICULTY[c.difficulty - 1]}</span
													>
												{/if}
												{#if c.moment}<span class="tag">As it happens</span>{/if}
												{#if c.earned}<span class="tag done"
														><Icon name="check" size={11} /> Earned</span
													>{/if}
											</span>
										</span>
									</span>
									{#if c.description}<span class="about">{c.description}</span>{/if}
									<span class="foot">
										{#if c.earned && c.held}
											<span>
												{c.held.automatic ? 'Earned' : 'Given'}
												{c.held.earned_on ? day(c.held.earned_on) : prettyDate(c.held.awarded_at)}
												{#if c.held.earned_with}<span class="faint with"
														>· with {c.held.earned_with}</span
													>{/if}
											</span>
										{:else if c.progress}
											<span class="progress">
												<span
													class="meter small"
													role="progressbar"
													aria-label="{c.name}: {count(c.progress.have)} of {count(
														c.progress.need
													)}"
													aria-valuemin={0}
													aria-valuemax={c.progress.need}
													aria-valuenow={c.progress.have}
												>
													<span style:width="{share(c) * 100}%"></span>
												</span>
												<span class="numbers"
													>{count(c.progress.have)} / {count(c.progress.need)}</span
												>
											</span>
										{:else}
											<span class="faint">Still to earn</span>
										{/if}
										<span class="faint held"
											><Icon name="users" size={12} /> {heldBy(c.holders, accounts)}</span
										>
									</span>
								</button>
							</li>
						{/each}
					</ul>
				</section>
			{/each}

			{#if open}
				<BadgeDetail
					card={open}
					ladder={ladderOf(open)}
					{accounts}
					own={profile.me}
					pinnable={tops.has(open.id)}
					pinned={beside.some((b) => b.id === open?.id)}
					onpick={(c) => (openId = c.id)}
					onclose={() => (openId = null)}
					onpin={(c, v) => setPinned(c.id, v)}
				/>
			{/if}
		{/if}
	{/snippet}
</ProfileShell>

<style>
	.summary {
		display: grid;
		grid-template-columns: minmax(0, 1.4fr) repeat(2, minmax(0, 1fr));
		align-items: center;
		gap: 16px;
		padding: 18px 20px;
		margin-bottom: 12px;
	}

	.tally {
		display: grid;
		gap: 8px;
	}

	.tally p {
		margin: 0;
		display: flex;
		align-items: baseline;
		gap: 6px;
	}

	.tally strong {
		font-family: var(--font-display);
		font-size: 1.8rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.dim {
		color: var(--text-dim);
	}

	.percent {
		font-size: 0.8rem;
	}

	.pick {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
		padding: 10px 12px;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--bg-elev);
		color: inherit;
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition:
			border-color var(--fast) var(--ease),
			transform var(--fast) var(--ease);
	}

	.pick:hover {
		border-color: var(--line-strong);
		transform: translateY(-1px);
	}

	.pick > span {
		display: grid;
		min-width: 0;
		font-size: 0.85rem;
	}

	.pick strong {
		font-size: 0.95rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.grey {
		filter: grayscale(1);
		opacity: 0.55;
	}

	.meter {
		display: block;
		height: 8px;
		border-radius: 999px;
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.meter > span {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--accent);
		transition: width 600ms var(--ease);
	}

	.meter.small {
		height: 5px;
		flex: 1;
	}

	.beside {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 14px 20px;
		margin-bottom: 12px;
	}

	.beside-text {
		display: grid;
		gap: 2px;
		min-width: 0;
	}

	.beside-text .faint {
		font-size: 0.84rem;
	}

	.slots {
		list-style: none;
		margin: 0;
		padding: 4px 0 0;
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 10px;
		max-width: 60%;
	}

	/* Past the first three: one tap away beside the name, so drawn quieter. */
	.slots li.extra .slot {
		opacity: 0.6;
		transform: scale(0.86);
	}

	.slots li {
		position: relative;
		width: 52px;
		height: 52px;
	}

	.slots li:global([data-dragging]) {
		z-index: 2;
		opacity: 0.85;
	}

	.slot {
		width: 52px;
		height: 52px;
		display: grid;
		place-items: center;
		padding: 0;
		border: 1px solid var(--line);
		border-radius: 50%;
		background: var(--bg-elev);
		cursor: grab;
		touch-action: none;
	}

	.slot:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.unpin {
		position: absolute;
		top: -4px;
		right: -4px;
		width: 20px;
		height: 20px;
		display: grid;
		place-items: center;
		padding: 0;
		border: 1px solid var(--line-strong);
		border-radius: 50%;
		background: var(--bg-float);
		color: var(--text-dim);
		cursor: pointer;
	}

	.empty {
		border: 1px dashed var(--line-strong);
		border-radius: 50%;
	}

	.tools {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		margin: 4px 0 8px;
	}

	.searchbar {
		flex: 1 1 220px;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 14px;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		color: var(--text-faint);
		transition: border-color var(--fast) var(--ease);
	}

	.searchbar:focus-within {
		border-color: var(--accent);
	}

	.searchbar input {
		flex: 1;
		min-width: 0;
		padding: 9px 0;
		background: none;
		border: none;
		outline: none;
		color: var(--text);
		font: inherit;
	}

	.none {
		margin: 16px 0 0;
	}

	.group header {
		margin: 26px 0 10px;
	}

	.group h2 {
		margin: 0;
		font-size: 1.05rem;
		font-weight: 650;
	}

	.group h2 .faint {
		font-weight: 500;
		margin-left: 4px;
	}

	.group header p {
		margin: 2px 0 0;
		font-size: 0.85rem;
	}

	.grid {
		list-style: none;
		margin: 12px 0 0;
		padding: 0;
		display: grid;
		/* Three at most, two on a narrower screen, one on a phone. */
		grid-template-columns: repeat(
			auto-fill,
			minmax(min(100%, max(320px, calc((100% - 24px) / 3))), 1fr)
		);
		gap: 12px;
	}

	.grid li {
		display: flex;
		animation: card-in 380ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		animation-delay: calc(var(--i) * 18ms);
	}

	@keyframes card-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}

	.badge-card {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-width: 0;
		padding: 14px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		color: inherit;
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition:
			border-color var(--fast) var(--ease),
			transform var(--fast) var(--ease),
			box-shadow var(--fast) var(--ease);
	}

	.badge-card:hover {
		border-color: var(--line-strong);
		transform: translateY(-2px);
		box-shadow: 0 10px 24px rgb(0 0 0 / 0.14);
	}

	.badge-card:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.top {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}

	.top img {
		flex: none;
		width: 64px;
		height: 64px;
		transition: transform var(--fast) var(--ease);
	}

	.badge-card:hover .top img {
		transform: scale(1.05) rotate(-3deg);
	}

	.locked .top img {
		filter: grayscale(1);
		opacity: 0.45;
	}

	.locked strong {
		color: var(--text-dim);
	}

	.names {
		display: grid;
		gap: 3px;
		min-width: 0;
	}

	.names strong {
		font-size: 1rem;
		overflow-wrap: anywhere;
	}

	.eyebrow {
		color: var(--text-faint);
		font-size: 0.74rem;
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	.tags {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}

	.tag {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 1px 7px;
		border-radius: 999px;
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-size: 0.7rem;
		font-weight: 600;
		white-space: nowrap;
	}

	.tag.done {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		color: var(--accent);
	}

	.difficulty.d4,
	.difficulty.d5 {
		color: var(--text);
	}

	.about {
		color: var(--text-dim);
		font-size: 0.88rem;
		line-height: 1.4;
	}

	.foot {
		margin-top: auto;
		display: grid;
		gap: 6px;
		padding-top: 10px;
		border-top: 1px solid var(--line);
		font-size: 0.8rem;
		color: var(--text-dim);
	}

	.with {
		overflow-wrap: anywhere;
	}

	.progress {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.numbers {
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.held {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.76rem;
	}

	@media (max-width: 760px) {
		.summary {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		}

		.tally {
			grid-column: 1 / -1;
		}
	}

	@media (max-width: 600px) {
		.summary {
			grid-template-columns: minmax(0, 1fr);
			padding: 14px 16px;
		}

		.beside {
			padding: 12px 16px;
			flex-direction: column;
			align-items: stretch;
		}

		.slots {
			max-width: none;
			justify-content: flex-start;
		}

		.searchbar {
			flex-basis: 100%;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.grid li {
			animation: none;
		}

		.badge-card,
		.badge-card:hover,
		.badge-card:hover .top img {
			transform: none;
		}
	}
</style>
