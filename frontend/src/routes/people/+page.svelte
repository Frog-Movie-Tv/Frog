<script lang="ts">
	import { api } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import type { PersonSummary } from '#lib/types';

	let profiles = $state<PersonSummary[]>([]);
	let loading = $state(true);

	$effect(() => {
		if (!session.authenticated) return;
		void api
			.profiles()
			.then((r) => (profiles = r.profiles))
			.catch(() => (profiles = []))
			.finally(() => (loading = false));
	});

	const others = $derived(profiles.filter((p) => !p.me));
</script>

<svelte:head>
	<title>People · Frog</title>
</svelte:head>

<div class="page">
	<h1>People</h1>
	<p class="muted intro">Everyone on this Frog who shows their profile.</p>

	{#if loading}
		<Spinner label="Loading people" />
	{:else if others.length === 0}
		<Empty
			art="rest"
			title="Nobody is sharing yet"
			hint="When someone turns on their profile in Settings, they appear here."
		>
			<a class="btn primary" href="/settings#account">Share my profile</a>
		</Empty>
	{:else}
		<ul class="people">
			{#each others as person (person.username)}
				<li>
					<a class="card person" href="/people/{encodeURIComponent(person.username)}">
						<Avatar
							src={person.avatar
								? `/api/profiles/${encodeURIComponent(person.username)}/images/avatar`
								: null}
							name={person.username}
							size={44}
						/>
						<span class="who">
							<strong>{person.display_name ?? person.username}</strong>
							{#if person.display_name}<span class="faint">@{person.username}</span>{/if}
						</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.intro {
		margin: 6px 0 22px;
	}

	.people {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 220px), 1fr));
		gap: 12px;
	}

	.person {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 14px 16px;
		color: inherit;
		text-decoration: none;
	}

	.person:hover {
		border-color: var(--line-strong);
	}

	.who {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.who .faint {
		font-size: 0.8rem;
	}
</style>
