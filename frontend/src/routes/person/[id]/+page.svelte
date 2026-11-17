<script lang="ts">
	import { titleKey } from '#lib/titles';
	import { uniqueBy } from '#lib/unique';
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import { titleMenu } from '#lib/menus';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { prettyDate } from '#lib/format';
	import { addMany, chooseStatus } from '#lib/add';
	import Icon from '#lib/components/Icon.svelte';
	import PickBar from '#lib/components/PickBar.svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import type { PersonDetail, SearchItem } from '#lib/types';

	let person = $state<PersonDetail | null>(null);
	let loading = $state(true);
	let error = $state<Failure | null>(null);
	let seenOnly = $state(false);
	let expanded = $state(false);

	const id = $derived(Number(page.params.id));

	$effect(() => {
		void load(id);
	});

	let token = 0;

	/** A right-click's offer on a credit; the page reads itself again after. None while choosing. */
	const menuOf = (item: SearchItem) =>
		selecting
			? undefined
			: titleMenu({ ...item, status: item.in_library }, { onchanged: () => load(id) });

	async function load(personId: number) {
		const mine = ++token;
		loading = true;
		error = null;
		try {
			const res = await api.person(personId);
			if (mine !== token) return;
			person = res;
		} catch (e) {
			if (mine !== token) return;
			error = failure(e, {
				title: "This person isn't on TMDB",
				hint: 'The link may be wrong, or TMDB has removed them.'
			});
		} finally {
			if (mine === token) loading = false;
		}
	}

	const shown = $derived(
		uniqueBy(person?.credits ?? [], (c) => titleKey(c.item)).filter(
			(c) => !seenOnly || c.item.in_library !== null
		)
	);
	const credits = $derived(shown.filter((c) => !c.appearance));
	const appearances = $derived(shown.filter((c) => c.appearance));
	const seenCount = $derived(
		(person?.credits ?? []).filter((c) => c.item.in_library !== null).length
	);

	let selecting = $state(false);
	let adding = $state(false);
	const picked = new SvelteSet<string>();
	const free = $derived(shown.map((c) => c.item).filter((t) => !t.in_library));
	const chosen = $derived(free.filter((t) => picked.has(titleKey(t))));

	function toggle(item: SearchItem) {
		const key = titleKey(item);
		if (picked.has(key)) picked.delete(key);
		else picked.add(key);
	}

	function selectAll() {
		const every = chosen.length === free.length;
		picked.clear();
		if (!every) for (const t of free) picked.add(titleKey(t));
	}

	function stopSelecting() {
		selecting = false;
		picked.clear();
	}

	/** What makes a poster a tick box while choosing. */
	function pickable(item: SearchItem) {
		if (!selecting) return {};
		const held = !!item.in_library;
		return {
			role: 'checkbox',
			'aria-checked': held ? undefined : picked.has(titleKey(item)),
			'aria-disabled': held ? true : undefined,
			onclick: (e: MouseEvent) => {
				e.preventDefault();
				e.stopPropagation();
				if (!held) toggle(item);
			},
			onkeydown: (e: KeyboardEvent) => {
				if (e.key !== ' ') return;
				e.preventDefault();
				if (!held) toggle(item);
			}
		};
	}

	/** Every ticked title into the library, with the status chosen for all of them. */
	async function addChosen(e: MouseEvent) {
		const list = chosen;
		if (!list.length) return;
		const naming = list.length === 1 ? list[0].name : `${list.length} titles`;
		const status = await chooseStatus(
			{ name: naming, kind: list.every((t) => t.kind === 'movie') ? 'movie' : 'tv' },
			e.currentTarget as HTMLElement,
			{ event: e, title: `Add ${naming}` }
		);
		if (!status) return;
		adding = true;
		try {
			const done = await addMany(list, status, () => load(id));
			if (done) picked.clear();
		} finally {
			adding = false;
		}
	}

	/** Two parts and a count: one voice actor can have dozens in one show. */
	function roleLine(role: string): string {
		const parts = role.split(' / ');
		if (parts.length <= 2) return role;
		return `${parts.slice(0, 2).join(' / ')} +${parts.length - 2} more`;
	}
</script>

<svelte:head>
	<title>
		{person?.name ? `${person.name} · Frog` : error ? `${error.heading} · Frog` : 'Frog'}
	</title>
</svelte:head>

<div class="page">
	{#if loading && !person}
		<Spinner label="Loading" />
	{:else if error}
		<Empty art={error.missing ? 'lost' : 'failed'} title={error.title} hint={error.hint}>
			{#if !error.missing}
				<button class="btn primary" onclick={() => load(id)}>Try again</button>
			{/if}
			<a class="btn" class:primary={error.missing} href="/">Go home</a>
		</Empty>
	{:else if person}
		<header class="head card">
			<div class="photo">
				{#if person.profile_path}
					<img src={img(person.profile_path, 'w342')} alt="Portrait of {person.name}" />
				{:else}
					<span aria-hidden="true">{person.name?.slice(0, 1) ?? '?'}</span>
				{/if}
			</div>

			<div class="info">
				<h1>{person.name}</h1>
				<p class="facts">
					{#if person.known_for_department}
						<span class="pill">{person.known_for_department}</span>
					{/if}
					{#if person.birthday}
						<span class="pill"
							>{prettyDate(person.birthday)}{person.deathday
								? ` – ${prettyDate(person.deathday)}`
								: ''}</span
						>
					{/if}
					{#if person.place_of_birth}
						<span class="pill">{person.place_of_birth}</span>
					{/if}
				</p>

				{#if person.biography}
					<p class="bio" class:clamped={!expanded}>{person.biography}</p>
					<button class="btn ghost small" onclick={() => (expanded = !expanded)}>
						{expanded ? 'Show less' : 'Read more'}
					</button>
				{/if}
			</div>
		</header>

		<div class="section-head">
			<h2>Filmography</h2>
			<div class="tools">
				<div class="segmented" role="group" aria-label="Filmography">
					<button class="chip" class:on={!seenOnly} onclick={() => (seenOnly = false)}>All</button>
					<button class="chip" class:on={seenOnly} onclick={() => (seenOnly = true)}>
						In your library {seenCount}
					</button>
				</div>
				{#if free.length && !selecting}
					<button class="btn ghost small" onclick={() => (selecting = true)}>
						<Icon name="check" size={14} /> Select
					</button>
				{/if}
			</div>
		</div>

		{#if !shown.length}
			<Empty
				title={seenOnly ? 'Nothing of theirs is in your library yet' : 'No credits found'}
				hint={seenOnly ? 'Turn the filter off to see everything.' : undefined}
			/>
		{:else}
			<Grid>
				{#each credits as credit (titleKey(credit.item))}
					<div class="credit" class:seen={selecting || credit.item.in_library !== null}>
						<Poster
							{...pickable(credit.item)}
							morph={!selecting}
							selected={selecting && !credit.item.in_library
								? picked.has(titleKey(credit.item))
								: undefined}
							menu={menuOf(credit.item)}
							kind={credit.item.kind}
							ref={credit.item}
							name={credit.item.name}
							posterPath={credit.item.poster_path}
							fallbackPath={credit.item.backdrop_path}
							year={credit.item.year}
							status={credit.item.in_library}
						/>
						{#if credit.role}
							<span class="role faint" title={credit.role}>{roleLine(credit.role)}</span>
						{/if}
					</div>
				{/each}
			</Grid>
		{/if}

		{#if appearances.length}
			<div class="appearances">
				<Rail title="Appearances">
					{#each appearances as credit (titleKey(credit.item))}
						<div class="credit" class:seen={selecting || credit.item.in_library !== null}>
							<Poster
								{...pickable(credit.item)}
								morph={!selecting}
								selected={selecting && !credit.item.in_library
									? picked.has(titleKey(credit.item))
									: undefined}
								menu={menuOf(credit.item)}
								kind={credit.item.kind}
								ref={credit.item}
								name={credit.item.name}
								posterPath={credit.item.poster_path}
								fallbackPath={credit.item.backdrop_path}
								year={credit.item.year}
								status={credit.item.in_library}
							/>
						</div>
					{/each}
				</Rail>
			</div>
		{/if}
	{/if}
</div>

{#if selecting}
	<PickBar
		count={chosen.length}
		selectable={free.length}
		busy={adding}
		onselectall={selectAll}
		onadd={addChosen}
		ondone={stopSelecting}
	/>
{/if}

<style>
	/* One pane: the portrait, the name as a page's title, the facts as pills. */
	.head {
		display: flex;
		gap: 26px;
		margin-bottom: 40px;
		padding: 22px;
		align-items: flex-start;
	}

	.photo {
		flex: none;
		width: 168px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		display: grid;
		place-items: center;
		font-size: 2rem;
		color: var(--text-faint);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-2);
	}

	.photo img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.info {
		display: flex;
		flex-direction: column;
		gap: 12px;
		align-items: flex-start;
		min-width: 0;
	}

	.info h1 {
		font-size: clamp(2rem, 4vw, 3rem);
		letter-spacing: -0.035em;
		line-height: 1.05;
	}

	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 0;
	}

	.bio {
		max-width: 76ch;
		font-size: 0.92rem;
		color: var(--text-dim);
		white-space: pre-line;
	}

	.bio.clamped {
		display: -webkit-box;
		-webkit-line-clamp: 4;
		line-clamp: 4;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.btn.small {
		padding: 6px 12px;
		font-size: 0.78rem;
	}

	.credit {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	/* Everything they made, with what I have seen pulled forward. */
	.credit:not(.seen) {
		opacity: 0.56;
		transition: opacity var(--fast) var(--ease);
	}

	.credit:not(.seen):hover {
		opacity: 1;
	}

	.role {
		font-size: 0.72rem;
		line-height: 1.3;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	@media (max-width: 720px) {
		.head {
			flex-direction: column;
			gap: 16px;
			padding: 16px;
		}

		.photo {
			width: 120px;
		}
	}

	.appearances {
		margin-top: 44px;
	}

	.tools {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 8px;
	}

	/* Room under the grid for the bar while choosing. */
	.page:has(~ :global(.bar)) {
		padding-bottom: 90px;
	}
</style>
