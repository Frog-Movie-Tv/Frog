<script lang="ts" module>
	/** Not reactive on purpose: a plain note of what was last read. */
	const cache: Record<string, Profile> = {};
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import Empty from './Empty.svelte';
	import LoadFailed from './LoadFailed.svelte';
	import ProfileHeader, { type ProfileTab } from './ProfileHeader.svelte';
	import Spinner from './Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { session } from '#lib/session.svelte';
	import type { Profile } from '#lib/types';

	interface Props {
		tab: ProfileTab;
		/** What the tab shows under the header, once the profile is in. */
		children: Snippet<[Profile]>;
	}

	let { tab, children }: Props = $props();

	const username = $derived(page.params.username ?? '');

	/*
	 * Every tab of a profile is its own page with the same header. The last
	 * profile read is kept here, so moving between tabs draws the header at
	 * once and only the tab's own part waits.
	 */
	let profile = $state<Profile | null>(null);
	let loading = $state(true);
	let failed = $state<Failure | null>(null);

	$effect(() => {
		if (session.authenticated && username) load(username);
	});

	let token = 0;

	function load(name: string) {
		const mine = ++token;
		const kept = cache[name.toLowerCase()];
		if (kept) profile = kept;
		loading = !kept;
		failed = null;
		void api
			.profile(name)
			.then((p) => {
				if (mine !== token) return;
				profile = p;
				cache[name.toLowerCase()] = p;
			})
			.catch((e) => {
				if (mine !== token) return;
				profile = null;
				failed = failure(e, {
					title: 'No profile here',
					hint: 'It may be private, or there is no such account on this Frog.'
				});
			})
			.finally(() => {
				if (mine === token) loading = false;
			});
	}

	const shownName = $derived(profile?.display_name || profile?.username || '');
	const label: Record<ProfileTab, string> = {
		overview: '',
		library: ' · Library',
		ratings: ' · Ratings',
		compare: ' · Compare'
	};
</script>

<svelte:head>
	<title>
		{profile
			? `${shownName}${label[tab]} · Frog`
			: failed?.missing
				? 'Not found · Frog'
				: 'Profile · Frog'}
	</title>
</svelte:head>

<div class="page">
	{#if loading && !profile}
		<Spinner label="Loading profile" />
	{:else if failed && !failed.missing}
		<LoadFailed {failed} onretry={() => load(username)} />
	{:else if failed || !profile}
		<Empty
			art="search"
			title="No profile here"
			hint="It may be private, or there is no such account on this Frog."
		>
			<a class="btn" href="/people">Everyone sharing</a>
		</Empty>
	{:else}
		<ProfileHeader {profile} {tab} />
		<div class="tab-body">
			{@render children(profile)}
		</div>
	{/if}
</div>

<style>
	.tab-body {
		animation: tab-in 320ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
	}

	@keyframes tab-in {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.tab-body {
			animation: none;
		}
	}
</style>
