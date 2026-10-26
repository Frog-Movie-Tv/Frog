<script lang="ts">
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';

	let profiles = $state<{ username: string; me: boolean }[]>([]);
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
			<a class="btn" href="/settings">Your profile setting</a>
		</Empty>
	{:else}
		<ul class="people">
			{#each others as person (person.username)}
				<li>
					<a class="card person" href="/people/{encodeURIComponent(person.username)}">
						<span class="avatar" aria-hidden="true"
							>{person.username.slice(0, 1).toUpperCase()}</span
						>
						<strong>{person.username}</strong>
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

	.avatar {
		width: 40px;
		height: 40px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: var(--accent-soft);
		color: var(--accent);
		font-weight: 700;
	}
</style>
