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
</script>

<section class="block" id="this-instance">
	<div class="section-head"><h2>This instance</h2></div>

	<div class="instance card">
		<Logo size={38} />
		<dl class="facts">
			<div>
				<dt>Signed in as</dt>
				<dd>{session.status?.username ?? '—'}</dd>
			</div>
			<div>
				<dt>Version</dt>
				<dd>{session.status?.version ?? '—'}</dd>
			</div>
			<div>
				<dt>Provider region</dt>
				<dd>{session.status?.region ?? '—'}</dd>
			</div>
			<div>
				<dt>TMDB key</dt>
				<dd class={session.status?.tmdb_configured ? 'ok' : 'bad'}>
					{session.status?.tmdb_configured ? 'configured' : 'missing'}
				</dd>
			</div>
		</dl>
	</div>

	<div class="actions">
		<a class="btn" href="/whats-new">Release notes</a>
		<a class="btn ghost" href="/about">About</a>
		<button class="btn ghost" onclick={checkForUpdates} disabled={update.state === 'checking'}>
			{update.state === 'checking' ? 'Checking…' : 'Check for updates'}
		</button>
		<button class="btn ghost" onclick={signOut}>Sign out</button>
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
		margin-bottom: 44px;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
		align-items: center;
	}

	.instance {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 18px;
		margin-bottom: 14px;
	}

	.facts {
		flex: 1;
		margin: 0;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
		gap: 10px 20px;
	}

	.facts div {
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 0.86rem;
	}

	dt {
		color: var(--text-dim);
		font-size: 0.76rem;
	}

	dd {
		margin: 0;
		font-weight: 600;
	}

	dd.ok {
		color: var(--good);
	}
	dd.bad {
		color: var(--bad);
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

	@media (max-width: 720px) {
		.instance {
			flex-direction: column;
			align-items: flex-start;
		}
	}
</style>
