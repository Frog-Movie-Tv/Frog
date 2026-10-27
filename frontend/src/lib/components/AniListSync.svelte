<script lang="ts">
	import { anilist, describe } from '#lib/import/anilist.svelte';
	import { plural, relativeDate } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';

	const linked = $derived(settings.current.anilistUser ?? '');
	let username = $state(settings.current.anilistUser ?? '');

	async function syncNow(event: SubmitEvent) {
		event.preventDefault();
		try {
			const outcome = await anilist.run(username);
			toasts.push(describe(outcome));
		} catch (e) {
			toasts.error(e);
		}
	}

	function stop() {
		settings.patch({ anilistUser: undefined, anilistSyncedAt: undefined });
		anilist.last = null;
		username = '';
		toasts.push('AniList sync is off');
	}
</script>

<div class="anilist card">
	<div class="label">
		<strong>Keep in step with AniList</strong>
		<span class="faint">
			{#if linked && settings.current.anilistSyncedAt}
				Synced {relativeDate(settings.current.anilistSyncedAt)} with {linked}. New titles and
				progress come across when Frog opens; your own ratings and statuses stay.
			{:else}
				New titles and progress from a public AniList list, whenever Frog opens.
			{/if}
		</span>
	</div>

	<form onsubmit={syncNow}>
		<input
			class="input"
			bind:value={username}
			placeholder="AniList username"
			aria-label="AniList username"
			autocomplete="off"
			autocapitalize="off"
			spellcheck="false"
		/>
		<button class="btn primary" disabled={anilist.running || !username.trim()}>
			{anilist.running ? 'Syncing…' : 'Sync now'}
		</button>
		{#if linked}
			<button type="button" class="btn ghost" disabled={anilist.running} onclick={stop}>
				Stop
			</button>
		{/if}
	</form>

	{#if anilist.last?.unsure}
		<p class="faint unsure">
			{plural(anilist.last.unsure, 'title')}
			{anilist.last.unsure === 1 ? 'needs' : 'need'} a choice first.
			<a href="/import">Review them on Import</a>
		</p>
	{/if}
</div>

<style>
	.anilist {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 15px 18px;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.label .faint {
		font-size: 0.82rem;
	}

	form {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	form .input {
		flex: 1 1 200px;
		width: auto;
	}

	.unsure {
		margin: 0;
		font-size: 0.82rem;
	}

	.unsure a {
		color: var(--accent);
	}
</style>
