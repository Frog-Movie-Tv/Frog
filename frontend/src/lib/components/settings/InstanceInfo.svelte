<script lang="ts">
	import { goto } from '$app/navigation';
	import Logo from '#lib/components/Logo.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';

	/** This instance: who is signed in, what is running, and whether it is the newest. */
	let update = $state<{
		state: 'idle' | 'checking' | 'current' | 'available' | 'failed';
		tag?: string;
		url?: string;
	}>({
		state: 'idle'
	});

	/**
	 * Deliberately a button rather than something automatic: Frog otherwise
	 * talks to nobody but TMDB, and a silent call home would undercut that.
	 */
	async function checkForUpdates() {
		update = { state: 'checking' };
		try {
			const res = await fetch('https://api.github.com/repos/Frog-Movie-Tv/Frog/releases/latest', {
				headers: { accept: 'application/vnd.github+json' }
			});
			if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
			const release = await res.json();
			const tag = String(release.tag_name ?? '').replace(/^v/, '');
			const here = session.status?.version ?? '';
			update =
				tag && tag !== here
					? { state: 'available', tag, url: release.html_url }
					: { state: 'current', tag };
		} catch (e) {
			update = { state: 'failed' };
			toasts.error(e);
		}
	}

	async function signOut() {
		await session.logout();
		toasts.push('Signed out');
		await goto('/login', { replaceState: true });
	}

	const status = $derived(session.status);
</script>

<section class="block" id="this-instance">
	<div class="section-head"><h2>This instance</h2></div>

	<div class="instance list-pane">
		<div class="row">
			<Logo size={34} />
			<span class="label">
				<strong>Frog {status?.version ?? ''}</strong>
				<span class="faint"
					>Signed in as {status?.username ?? '—'}{status?.region
						? ` · Region ${status.region}`
						: ''}</span
				>
			</span>
			<button class="btn ghost small" onclick={signOut}>Sign out</button>
		</div>
		{#if status && !status.tmdb_configured}
			<p class="missing">TMDB key missing: Frog cannot find titles until one is set.</p>
		{/if}
	</div>

	<div class="links">
		<a href="/whats-new">What’s new</a>
		<a href="/about">About</a>
		<button onclick={checkForUpdates} disabled={update.state === 'checking'}>
			{update.state === 'checking' ? 'Checking…' : 'Check for updates'}
		</button>
	</div>

	{#if update.state === 'available'}
		<p class="update available">
			<strong>Version {update.tag} is out.</strong>
			<a href={update.url} target="_blank" rel="noreferrer noopener">Read the notes</a>
			— Frog installs it on its own, usually within the hour.
		</p>
	{:else if update.state === 'current'}
		<p class="update faint">You are on the newest release.</p>
	{:else if update.state === 'failed'}
		<p class="update faint">Could not reach GitHub. Not to worry, nothing is wrong.</p>
	{/if}
</section>

<style>
	.block {
		margin-bottom: 36px;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 14px 18px;
	}

	.label {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.78rem;
	}

	.btn.small {
		flex: none;
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.missing {
		margin: 0;
		padding: 10px 18px 14px;
		font-size: 0.82rem;
		color: var(--bad);
	}

	/* Quiet links under the card: these are rarely the reason to be here. */
	.links {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 18px;
		margin-top: 12px;
		padding: 0 4px;
		font-size: 0.84rem;
	}

	.links a,
	.links button {
		padding: 0;
		border: 0;
		background: none;
		color: var(--text-dim);
		font: inherit;
		cursor: pointer;
	}

	.links a:hover,
	.links button:hover:not(:disabled) {
		color: var(--text);
		text-decoration: underline;
	}

	.update {
		margin-top: 12px;
		font-size: 0.86rem;
	}

	.update.available {
		padding: 11px 14px;
		border-radius: var(--radius-sm);
		background: var(--accent-soft);
		border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
	}

	.update a {
		color: var(--accent);
		text-decoration: underline;
	}
</style>
