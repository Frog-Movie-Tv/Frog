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
	import { profileStyle } from '#lib/profile-colours';
	import { currentTone } from '#lib/settings.svelte';
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
	/*
	 * The profile's own colour, on this page only and for everyone who opens
	 * it: the accent tokens are scoped here, never on the app's theme.
	 */
	const colour = $derived(
		profile ? profileStyle(profile.profile_color, profile.profile_shade, currentTone()) : undefined
	);
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

<div class="page" class:coloured={!!colour} style={colour}>
	{#if loading && !profile}
		<Spinner label="Loading profile" />
	{:else if failed && !failed.missing}
		<LoadFailed {failed} onretry={() => load(username)} />
	{:else if failed || !profile}
		<Empty
			art="lost"
			title="No profile here"
			hint="It may be private, or there is no such account on this Frog."
		>
			<a class="btn" href="/people">Everyone sharing</a>
		</Empty>
	{:else}
		<div class="profile" class:vertical={profile.banner_layout === 'vertical'}>
			<ProfileHeader {profile} {tab} />
			<div class="tab-body">
				{@render children(profile)}
			</div>
		</div>
	{/if}
</div>

<style>
	/* Vertical on a wide screen: the banner column on the left, the tabs and
	   what they show beside it (ProfileHeader puts its parts in the areas). */
	@media (min-width: 900px) {
		.profile.vertical {
			display: grid;
			grid-template-columns: minmax(240px, 330px) minmax(0, 1fr);
			/* The page's own bottom space is a row of the grid here, so the
			   column has room to stay put right to the end of the page. */
			grid-template-rows: auto 1fr 96px;
			grid-template-areas:
				'side tabs'
				'side body'
				'side .';
			column-gap: clamp(20px, 3vw, 40px);
			align-items: start;
		}

		.page:has(> .profile.vertical) {
			padding-bottom: 0;
		}

		.profile.vertical .tab-body {
			grid-area: body;
			min-width: 0;
		}
	}

	.tab-body {
		animation: tab-in 320ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
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
