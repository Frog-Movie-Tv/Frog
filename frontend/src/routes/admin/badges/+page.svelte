<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { api, ApiError } from '#lib/api';
	import AdminBadges from '#lib/components/AdminBadges.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import type { AdminUser } from '#lib/types';

	let users = $state<AdminUser[]>([]);
	let loading = $state(true);
	let denied = $state(false);
	let failed = $state(false);
	let panel = $state<ReturnType<typeof AdminBadges> | null>(null);

	onMount(async () => {
		try {
			users = (await api.adminUsers()).users;
		} catch (e) {
			denied = e instanceof ApiError && e.status === 403;
			failed = !denied;
		} finally {
			loading = false;
		}
	});

	// People's "Badges" button lands here with `?person=`: open their picker.
	let opened = false;
	$effect(() => {
		if (opened || !panel || !users.length) return;
		const id = Number(page.url.searchParams.get('person'));
		const user = users.find((u) => u.id === id);
		if (user) panel.forPerson(user);
		opened = true;
	});
</script>

<svelte:head><title>Badges · Frog</title></svelte:head>

<div class="page">
	<header class="page-head">
		<div>
			<h1>Badges</h1>
			<p class="sub">
				<a href="/admin">Administration</a> · Make badges, choose how they are earned, and give them out.
			</p>
		</div>
	</header>

	{#if loading}
		<Spinner label="Loading badges" />
	{:else if denied}
		<Empty
			art="none"
			title="This page is for administrators"
			hint="Ask whoever runs this instance if you need it."
		/>
	{:else if failed}
		<Empty art="none" title="Badges could not be loaded" hint="Try again in a minute." />
	{:else}
		<AdminBadges bind:this={panel} {users} />
	{/if}
</div>

<style>
	.sub a {
		color: var(--text);
	}
</style>
