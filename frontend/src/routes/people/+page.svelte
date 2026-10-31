<script lang="ts">
	import { api, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import type { PersonResult, PersonSummary } from '#lib/types';

	let profiles = $state<PersonSummary[]>([]);
	let loading = $state(true);
	let term = $state('');
	let crew = $state<PersonResult[]>([]);
	let looked = $state('');

	$effect(() => {
		if (!session.authenticated) return;
		void api
			.profiles()
			.then((r) => (profiles = r.profiles))
			.catch(() => (profiles = []))
			.finally(() => (loading = false));
	});

	// Film and TV people come from a search; members are already here.
	$effect(() => {
		const q = term.trim();
		if (q.length < 2) {
			crew = [];
			looked = '';
			return;
		}
		const timer = setTimeout(() => {
			void api
				.searchMore(q)
				.then((r) => {
					if (term.trim() === q) {
						crew = r.people;
						looked = q;
					}
				})
				.catch(() => (crew = []));
		}, 280);
		return () => clearTimeout(timer);
	});

	const plain = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '');
	const q = $derived(term.trim());
	const others = $derived(profiles.filter((p) => !p.me));
	const members = $derived(
		q
			? others.filter(
					(m) =>
						plain(m.username).includes(plain(q)) || plain(m.display_name ?? '').includes(plain(q))
				)
			: others
	);
	const nobody = $derived(!!q && looked === q && !members.length && !crew.length);
</script>

<svelte:head>
	<title>People · Frog</title>
</svelte:head>

{#snippet member(person: PersonSummary)}
	<li>
		<a class="person" href="/people/{encodeURIComponent(person.username)}">
			<Avatar
				src={person.avatar
					? `/api/profiles/${encodeURIComponent(person.username)}/images/avatar`
					: null}
				name={person.username}
				size={48}
			/>
			<span class="who">
				<strong>{person.display_name ?? person.username}</strong>
				<span class="faint">@{person.username}</span>
			</span>
		</a>
	</li>
{/snippet}

<div class="page">
	<div class="column">
		<h1>People</h1>

		<div class="searchbar">
			<Icon name="search" size={19} />
			<input
				bind:value={term}
				placeholder="Search people"
				aria-label="Search people"
				autocomplete="off"
				autocapitalize="off"
				spellcheck="false"
			/>
			{#if term}
				<button class="clear hit" aria-label="Clear search" onclick={() => (term = '')}>
					<Icon name="x" size={17} />
				</button>
			{/if}
		</div>

		{#if loading}
			<Spinner label="Loading people" />
		{:else if !q && others.length === 0}
			<Empty
				art="rest"
				title="Nobody is sharing yet"
				hint="When someone turns on their profile in Settings, they appear here."
			>
				<a class="btn primary" href="/settings#account">Share my profile</a>
			</Empty>
		{:else if nobody}
			<Empty
				art="search"
				title="Nobody by that name"
				hint="Check the spelling, or try a first name."
			/>
		{:else}
			{#if members.length}
				{#if q}<h2>On this Frog</h2>{/if}
				<ul class="people">
					{#each members as person (person.username)}{@render member(person)}{/each}
				</ul>
			{/if}
			{#if crew.length}
				<h2>Film and TV</h2>
				<ul class="people">
					{#each crew as p (p.id)}
						{@const src = img(p.profile_path, 'w185')}
						<li>
							<a class="person" href="/person/{p.id}">
								<span class="face">
									{#if src}<img {src} alt="" loading="lazy" />{:else}{p.name.slice(0, 1)}{/if}
								</span>
								<span class="who">
									<strong>{p.name}</strong>
									<span class="faint">{p.known_for.join(' · ') || p.department || ''}</span>
								</span>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
			{#if q}
				<a class="characters" href="/search?q={encodeURIComponent(q)}&type=characters">
					Looking for a character?
				</a>
			{/if}
		{/if}
	</div>
</div>

<style>
	.column {
		max-width: 640px;
		margin: 0 auto;
	}

	.searchbar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 16px;
		margin: 16px 0 20px;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		transition: border-color var(--fast) var(--ease);
	}

	.searchbar:focus-within {
		border-color: var(--accent);
	}

	.searchbar input {
		flex: 1;
		min-width: 0;
		padding: 14px 0;
		background: none;
		border: none;
		outline: none;
		color: var(--text);
		font: inherit;
	}

	.clear {
		display: grid;
		place-items: center;
		padding: 4px;
		background: none;
		border: none;
		color: var(--text-faint);
		cursor: pointer;
	}

	h2 {
		margin: 22px 0 6px;
		font-size: 0.86rem;
		color: var(--text-dim);
	}

	.people {
		display: grid;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.person {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 9px 8px;
		border-radius: var(--radius-sm);
		color: inherit;
		text-decoration: none;
	}

	.person:hover {
		background: var(--bg-elev);
	}

	.face {
		display: grid;
		place-items: center;
		flex: none;
		width: 48px;
		aspect-ratio: 2 / 3;
		overflow: hidden;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		color: var(--text-dim);
	}

	.face img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}

	.who {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.who .faint {
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		font-size: 0.8rem;
	}

	.characters {
		display: inline-block;
		margin-top: 18px;
		color: var(--accent);
		font-size: 0.86rem;
		font-weight: 600;
		text-decoration: none;
	}
</style>
