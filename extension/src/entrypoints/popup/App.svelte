<script lang="ts">
	import { browser } from 'wxt/browser';
	import { ask } from '../../lib/ask.ts';
	import Pip from '../../lib/Pip.svelte';
	import { label } from '../../lib/queue.ts';
	import * as store from '../../lib/storage.ts';
	import type { Candidate, Me, Pending, Resolution, TabView } from '../../lib/types.ts';

	const CONFIDENCE = { exact: 'Sure', likely: 'Likely', guess: 'A guess' } as const;

	let connected = $state<boolean | null>(null);
	let me = $state<Me | null>(null);
	let problem = $state('');
	let tab = $state<number | null>(null);
	let view = $state<TabView | null>(null);
	let queue = $state<Pending[]>([]);
	let busy = $state(false);

	// "What are you watching?"
	let searching = $state(false);
	let query = $state('');
	let results = $state<Candidate[]>([]);
	let picked = $state<Candidate | null>(null);
	let season = $state<number | null>(null);
	let episode = $state<number | null>(null);
	let looked = $state(false);

	async function load() {
		const s = await store.settings();
		connected = !!(s.url && s.token);
		if (!connected) return;
		// Opened as a page (`?tab=`) it reads that tab; as a popup, the one under it.
		const fromQuery = Number(new URLSearchParams(location.search).get('tab'));
		tab =
			fromQuery ||
			((await browser.tabs.query({ active: true, currentWindow: true }))[0]?.id ?? null);
		queue = await store.queue();
		try {
			me = await ask<Me>({ type: 'me' });
		} catch (e) {
			problem = e instanceof Error ? e.message : String(e);
		}
		if (tab != null) {
			view = await ask<TabView>({ type: 'view', tab });
			if (view.state === 'unknown' && !view.match) openSearch();
		}
	}
	load();

	function openSearch() {
		searching = true;
		query = view?.guess ?? '';
		season = view?.match?.season ?? null;
		episode = view?.match?.episode ?? null;
		results = view?.match ? [view.match, ...view.alternatives] : (view?.alternatives ?? []);
		picked = null;
		looked = false;
		if (query && !results.length) void search();
	}

	async function search(event?: SubmitEvent) {
		event?.preventDefault();
		if (!query.trim() || tab == null) return;
		busy = true;
		try {
			const answer = await ask<Resolution>({ type: 'search', tab, query });
			results = answer.match ? [answer.match, ...answer.alternatives] : answer.alternatives;
			looked = true;
		} catch (e) {
			problem = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	async function act(msg: Parameters<typeof ask>[0]) {
		busy = true;
		problem = '';
		try {
			view = await ask<TabView>(msg);
			searching = false;
		} catch (e) {
			problem = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
			queue = await store.queue();
		}
	}

	async function onQueue(msg: Parameters<typeof ask>[0]) {
		try {
			queue = await ask<Pending[]>(msg);
		} catch (e) {
			problem = e instanceof Error ? e.message : String(e);
		}
	}

	const waiting = $derived(queue.filter((q) => !q.failed));
	const failed = $derived(queue.filter((q) => q.failed));
	const m = $derived(view?.match ?? null);
	const canLog = $derived(
		!!picked && (picked.kind === 'movie' || (season != null && episode != null && episode > 0))
	);
</script>

<main>
	<header>
		<Pip size={30} />
		<div class="who">
			<strong>Frog</strong>
			{#if me}<span class="faint">{me.username}</span>{/if}
		</div>
		<button class="btn ghost small" onclick={() => browser.runtime.openOptionsPage()}
			>Settings</button
		>
	</header>

	{#if connected === false}
		<section class="card empty">
			<Pip size={56} />
			<h2>Connect to your Frog</h2>
			<p class="faint">Make a token in Frog under Settings → Connections, then paste it here.</p>
			<button class="btn primary" onclick={() => browser.runtime.openOptionsPage()}>Connect</button>
		</section>
	{:else if connected}
		{#if problem}<p class="problem" role="alert">{problem}</p>{/if}

		<section class="card now" data-testid="now">
			{#if m && !searching}
				<div class="line">
					<span class="faint small-caps">
						{view?.state === 'logged'
							? 'Logged'
							: view?.state === 'queued'
								? 'Waiting to reach Frog'
								: 'Playing'}
					</span>
					<span class="chip {m.confidence}">{CONFIDENCE[m.confidence]}</span>
				</div>
				<h2>{m.name}</h2>
				<p class="faint">
					{#if m.kind === 'movie'}Film{m.year
							? ` · ${m.year}`
							: ''}{:else if m.episode != null}S{m.season}E{m.episode}{m.episode_name
							? ` · ${m.episode_name}`
							: ''}{:else}Which episode?{/if}
				</p>
				{#if view?.progress != null}
					<div class="bar" aria-hidden="true">
						<span style:width="{Math.round(view.progress * 100)}%"></span>
					</div>
				{/if}
				<div class="actions">
					{#if view?.state === 'logged' || view?.state === 'queued'}
						<button class="btn" disabled={busy} onclick={() => act({ type: 'undo', tab: tab! })}
							>Undo</button
						>
					{:else if m.kind === 'movie' || m.episode != null}
						<button
							class="btn primary"
							disabled={busy}
							onclick={() => act({ type: 'confirm', tab: tab! })}>Log</button
						>
					{/if}
					<button class="btn ghost" onclick={openSearch}>Not this</button>
				</div>
			{:else if searching || view?.state === 'unknown'}
				<h2>What are you watching?</h2>
				<form class="search" onsubmit={search}>
					<input
						class="input"
						bind:value={query}
						placeholder="A film or a show"
						aria-label="Search"
					/>
					<button class="btn" disabled={busy || !query.trim()}>Search</button>
				</form>
				{#if results.length}
					<ul class="results">
						{#each results as r (`${r.title_id}-${r.tmdb?.kind}-${r.tmdb?.id}`)}
							<li>
								<button
									class="result"
									class:on={picked === r}
									onclick={() => {
										picked = r;
										if (r.kind === 'tv') season ??= 1;
									}}
								>
									<strong>{r.name}</strong>
									<span class="faint"
										>{r.kind === 'movie' ? 'Film' : 'Show'}{r.year
											? ` · ${r.year}`
											: ''}{r.in_library ? ' · In your library' : ''}</span
									>
								</button>
							</li>
						{/each}
					</ul>
				{:else if looked}
					<p class="faint">Nothing by that name. Try another.</p>
				{/if}
				{#if picked?.kind === 'tv'}
					<div class="numbers">
						<label class="field"
							>Season <input class="input" type="number" min="0" bind:value={season} /></label
						>
						<label class="field"
							>Episode <input class="input" type="number" min="1" bind:value={episode} /></label
						>
					</div>
				{/if}
				<div class="actions">
					<button
						class="btn primary"
						disabled={busy || !canLog}
						onclick={() => act({ type: 'choose', tab: tab!, candidate: picked!, season, episode })}
						>Log {picked
							? label(
									picked.name,
									picked.kind === 'tv' ? season : null,
									picked.kind === 'tv' ? episode : null
								)
							: ''}</button
					>
					{#if m}<button class="btn ghost" onclick={() => (searching = false)}>Back</button>{/if}
				</div>
				<p class="faint hint">Frog remembers your answer for {view?.domain ?? 'this site'}.</p>
			{:else}
				<p class="faint">Nothing playing here yet.</p>
			{/if}
		</section>

		{#if view?.domain}
			<label class="card site">
				<span>
					<strong>Detect on {view.domain}</strong>
					{#if view.hasRule}<span class="faint">Frog knows this site.</span>{/if}
				</span>
				<input
					type="checkbox"
					class="switch"
					checked={!view.ignored}
					onchange={(e) =>
						act({ type: 'site', tab: tab!, on: (e.currentTarget as HTMLInputElement).checked })}
				/>
			</label>
		{/if}

		{#if waiting.length || failed.length}
			<section class="card queue">
				{#if waiting.length}
					<div class="line">
						<span
							>{waiting.length === 1 ? 'One watch' : `${waiting.length} watches`} waiting to reach Frog</span
						>
						<button class="btn small" onclick={() => onQueue({ type: 'flush' })}>Send now</button>
					</div>
				{/if}
				{#each failed as f (f.id)}
					<div class="line">
						<span><strong>{f.label}</strong> <span class="faint">{f.failed}</span></span>
						<span class="row">
							<button class="btn small" onclick={() => onQueue({ type: 'retry', id: f.id })}
								>Retry</button
							>
							<button
								class="btn ghost small danger"
								onclick={() => onQueue({ type: 'discard', id: f.id })}>Discard</button
							>
						</span>
					</div>
				{/each}
			</section>
		{/if}

		{#if me?.watching.length}
			<section class="next">
				<h3>Up next</h3>
				<ul>
					{#each me.watching.slice(0, 6) as w (w.title_id)}
						<li>
							<span>{w.name}</span>
							<span class="faint"
								>{w.next ? `S${w.next.season_number}E${w.next.episode_number}` : 'Caught up'}</span
							>
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	{/if}
</main>

<style>
	main {
		width: 340px;
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	header {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.who {
		flex: 1;
		display: flex;
		flex-direction: column;
		line-height: 1.2;
	}

	.small {
		padding: 6px 10px;
		font-size: 12px;
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 8px;
		padding: 22px 16px;
	}

	.empty p {
		margin: 0;
	}

	.problem {
		margin: 0;
		padding: 8px 12px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--bad) 16%, transparent);
		color: var(--bad);
		font-size: 13px;
	}

	.now h2 {
		font-size: 1.1rem;
		margin: 6px 0 2px;
	}

	.now p {
		margin: 0;
	}

	.line {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.small-caps {
		font-size: 12px;
		font-weight: 650;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.bar {
		height: 4px;
		margin-top: 10px;
		border-radius: 4px;
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.actions {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}

	/* "Log" and a long name: this one may wrap rather than leave the popup. */
	.actions .btn.primary {
		flex: 0 1 auto;
		white-space: normal;
		line-height: 1.25;
		text-align: center;
	}

	.search {
		display: flex;
		gap: 8px;
		margin-top: 10px;
	}

	.results {
		list-style: none;
		margin: 10px 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
		max-height: 220px;
		overflow-y: auto;
	}

	.result {
		width: 100%;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		padding: 8px 10px;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		color: var(--text);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.result.on {
		border-color: var(--accent);
		background: var(--accent-soft);
	}

	.result .faint {
		font-size: 12px;
	}

	.numbers {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 8px;
		margin-top: 10px;
	}

	.hint {
		margin: 8px 0 0;
		font-size: 12px;
	}

	.site {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		cursor: pointer;
	}

	.site > span {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.site .faint {
		font-size: 12px;
	}

	.queue {
		display: flex;
		flex-direction: column;
		gap: 8px;
		font-size: 13px;
	}

	.row {
		display: flex;
		gap: 4px;
		flex: none;
	}

	.next h3 {
		font-size: 13px;
		color: var(--text-dim);
		margin: 4px 0 6px;
	}

	.next ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 13px;
	}

	.next li {
		display: flex;
		justify-content: space-between;
		gap: 8px;
	}
</style>
