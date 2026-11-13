<script lang="ts">
	import { browser } from 'wxt/browser';
	import { ask } from '../../lib/ask.ts';
	import Pip from '../../lib/Pip.svelte';
	import {
		checkRule,
		cleanDomain,
		describe,
		readExport,
		writeExport,
		type Rule,
		type Rules
	} from '../../lib/rules.ts';
	import * as store from '../../lib/storage.ts';
	import type { Me, Settings } from '../../lib/types.ts';

	let settings = $state<Settings | null>(null);
	let rules = $state<Rules>({});
	let url = $state('');
	let token = $state('');
	let me = $state<Me | null>(null);
	let status = $state('');
	let problem = $state('');
	let busy = $state(false);
	let newSite = $state('');

	/** The rule open for editing, as JSON text. */
	let editing = $state<{ domain: string; text: string; error: string } | null>(null);
	let copying = $state<{ domain: string; to: string; error: string } | null>(null);
	/** The last rule deleted, for Undo. */
	let deleted = $state<{ domain: string; rule: Rule } | null>(null);
	let undoTimer: ReturnType<typeof setTimeout> | undefined;

	async function load() {
		settings = await store.settings();
		rules = await store.rules();
		url = settings.url;
		if (settings.url && settings.token) {
			try {
				me = await ask<Me>({ type: 'me' });
			} catch (e) {
				problem = e instanceof Error ? e.message : String(e);
			}
		}
	}
	load();

	function connect(event: SubmitEvent) {
		event.preventDefault();
		// Asked straight away, inside the click: Firefox lets you choose which
		// sites an extension reads, and this one needs them all to find players.
		const granted = browser.permissions.request({ origins: ['<all_urls>'] }).catch(() => false);
		void finishConnect(granted);
	}

	async function finishConnect(granted: Promise<boolean>) {
		problem = '';
		status = '';
		const clean = store.cleanUrl(url);
		if (!clean) {
			problem = 'That is not an address.';
			return;
		}
		if (!(await granted)) {
			problem = 'Frog needs to read the sites you watch on to find what plays.';
			return;
		}
		busy = true;
		try {
			const previous = await store.settings();
			settings = await store.saveSettings({ url: clean, token: token.trim() || previous.token });
			url = clean;
			token = '';
			me = await ask<Me>({ type: 'me' });
			status = `Connected as ${me.username}.`;
		} catch (e) {
			me = null;
			problem = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	async function disconnect() {
		settings = await store.saveSettings({ token: '' });
		me = null;
		status = 'Disconnected. Revoke the token in Frog if you no longer need it.';
	}

	async function save(change: Partial<Settings>) {
		settings = await store.saveSettings(change);
	}

	async function addSite(event: SubmitEvent) {
		event.preventDefault();
		const domain = cleanDomain(newSite);
		if (!domain || !settings) return;
		await save({ ignored: [...new Set([...settings.ignored, domain])].sort() });
		newSite = '';
	}

	async function saveRules(next: Rules) {
		rules = next;
		await store.saveRules(next);
	}

	function saveEdit() {
		if (!editing) return;
		let parsed: unknown;
		try {
			parsed = JSON.parse(editing.text);
		} catch {
			editing.error = 'That is not valid JSON.';
			return;
		}
		const rule = checkRule(parsed);
		if (!rule) {
			editing.error = 'That is not a rule Frog understands.';
			return;
		}
		void saveRules({ ...rules, [editing.domain]: rule });
		editing = null;
	}

	function saveCopy() {
		if (!copying) return;
		const to = cleanDomain(copying.to);
		if (!to) {
			copying.error = 'That is not a domain.';
			return;
		}
		void saveRules({
			...rules,
			[to]: { ...rules[copying.domain], updated: new Date().toISOString() }
		});
		status = `${copying.domain}’s rule now also applies to ${to}.`;
		copying = null;
	}

	function remove(domain: string) {
		const { [domain]: rule, ...rest } = rules;
		if (!rule) return;
		deleted = { domain, rule };
		void saveRules(rest);
		clearTimeout(undoTimer);
		undoTimer = setTimeout(() => (deleted = null), 7000);
	}

	function undoRemove() {
		if (!deleted) return;
		void saveRules({ ...rules, [deleted.domain]: deleted.rule });
		deleted = null;
	}

	function exportRules() {
		const blob = new Blob([writeExport(rules)], { type: 'application/json' });
		const a = document.createElement('a');
		a.href = URL.createObjectURL(blob);
		a.download = 'frog-site-rules.json';
		a.click();
		setTimeout(() => URL.revokeObjectURL(a.href), 1000);
	}

	async function importRules(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		problem = '';
		try {
			const { rules: incoming, skipped } = readExport(await file.text());
			const count = Object.keys(incoming).length;
			await saveRules({ ...rules, ...incoming });
			status = `${count} ${count === 1 ? 'rule' : 'rules'} imported${skipped ? `, ${skipped} left out` : ''}.`;
		} catch (e) {
			problem = e instanceof Error ? e.message : String(e);
		}
	}

	const domains = $derived(Object.keys(rules).sort());
	const when = (iso: string) => new Date(iso).toLocaleDateString();
</script>

<main>
	<header>
		<Pip size={44} />
		<div>
			<h1>Frog</h1>
			<p class="faint">Logs what you watch on any site.</p>
		</div>
	</header>

	{#if status}<p class="note" role="status">{status}</p>{/if}
	{#if problem}<p class="problem" role="alert">{problem}</p>{/if}

	<section class="card">
		<h2>Your Frog</h2>
		{#if me}
			<p>Connected as <strong>{me.username}</strong> at {settings?.url}.</p>
		{:else}
			<p class="faint">
				In Frog, open Settings → Connections, make a token for this browser and paste it here.
			</p>
		{/if}
		<form class="connect" onsubmit={connect}>
			<label class="field"
				>Address <input
					class="input"
					name="url"
					bind:value={url}
					placeholder="https://frog.example.com"
					autocomplete="url"
					required
				/></label
			>
			<label class="field"
				>Token <input
					class="input"
					name="token"
					type="password"
					bind:value={token}
					placeholder={settings?.token ? 'Saved' : 'frog_…'}
					autocomplete="off"
				/></label
			>
			<div class="actions">
				{#if settings?.token}<button type="button" class="btn ghost" onclick={disconnect}
						>Disconnect</button
					>{/if}
				<button
					class="btn primary"
					disabled={busy || !url.trim() || (!token.trim() && !settings?.token)}
					>{me ? 'Save' : 'Connect'}</button
				>
			</div>
		</form>
	</section>

	{#if settings}
		<section class="card">
			<h2>Logging</h2>
			<label class="row">
				<span>
					<strong>Log at {settings.threshold}%</strong>
					<span class="faint">Or when the video ends, whichever comes first.</span>
				</span>
				<input
					type="range"
					min="50"
					max="100"
					step="5"
					value={settings.threshold}
					aria-label="How far in a video counts as watched"
					oninput={(e) => save({ threshold: Number((e.currentTarget as HTMLInputElement).value) })}
				/>
			</label>
			<label class="row">
				<span>
					<strong>Always ask first</strong>
					<span class="faint">Even when Frog is sure what is playing.</span>
				</span>
				<input
					type="checkbox"
					class="switch"
					checked={settings.askFirst}
					onchange={(e) => save({ askFirst: (e.currentTarget as HTMLInputElement).checked })}
				/>
			</label>
		</section>

		<section class="card">
			<h2>Ignored sites</h2>
			<p class="faint">Nothing is detected on these.</p>
			{#if settings.ignored.length}
				<ul class="list">
					{#each settings.ignored as domain (domain)}
						<li>
							<span>{domain}</span>
							<button
								class="btn ghost small"
								onclick={() => save({ ignored: settings!.ignored.filter((d) => d !== domain) })}
								>Remove</button
							>
						</li>
					{/each}
				</ul>
			{/if}
			<form class="inline" onsubmit={addSite}>
				<input class="input" bind:value={newSite} placeholder="example.com" aria-label="Site" />
				<button class="btn" disabled={!cleanDomain(newSite)}>Ignore</button>
			</form>
		</section>

		<section class="card">
			<div class="head">
				<h2>Site rules</h2>
				<div class="actions">
					<label class="btn ghost small"
						>Import<input
							type="file"
							accept="application/json,.json"
							hidden
							onchange={importRules}
						/></label
					>
					<button class="btn ghost small" disabled={!domains.length} onclick={exportRules}
						>Export</button
					>
				</div>
			</div>
			<p class="faint">
				When you confirm or correct a match, Frog remembers where that site keeps the title and the
				episode. If a site moves to a new address, copy its rule there.
			</p>
			{#if deleted}
				<p class="note">
					{deleted.domain}’s rule deleted. <button class="link" onclick={undoRemove}>Undo</button>
				</p>
			{/if}
			{#if domains.length}
				<ul class="list rules">
					{#each domains as domain (domain)}
						{@const rule = rules[domain]!}
						<li>
							<div class="rule-head">
								<span>
									<strong>{domain}</strong>
									<span class="faint">{describe(rule)} · {when(rule.updated)}</span>
								</span>
								<span class="actions">
									<button
										class="btn ghost small"
										onclick={() =>
											(editing = {
												domain,
												text: JSON.stringify(rules[domain], null, 2),
												error: ''
											})}>Edit</button
									>
									<button
										class="btn ghost small"
										onclick={() => (copying = { domain, to: '', error: '' })}>Copy to…</button
									>
									<button class="btn ghost small danger" onclick={() => remove(domain)}
										>Delete</button
									>
								</span>
							</div>
							{#if editing?.domain === domain}
								<textarea class="input code" rows="10" bind:value={editing.text}></textarea>
								{#if editing.error}<p class="problem">{editing.error}</p>{/if}
								<div class="actions">
									<button class="btn ghost small" onclick={() => (editing = null)}>Cancel</button>
									<button class="btn primary small" onclick={saveEdit}>Save</button>
								</div>
							{/if}
							{#if copying?.domain === domain}
								<form
									class="inline"
									onsubmit={(e) => {
										e.preventDefault();
										saveCopy();
									}}
								>
									<input
										class="input"
										bind:value={copying.to}
										placeholder="new-address.example"
										aria-label="New domain"
									/>
									<button class="btn small">Copy</button>
									<button type="button" class="btn ghost small" onclick={() => (copying = null)}
										>Cancel</button
									>
								</form>
								{#if copying.error}<p class="problem">{copying.error}</p>{/if}
							{/if}
						</li>
					{/each}
				</ul>
			{:else}
				<p class="faint">None yet.</p>
			{/if}
		</section>

		<section class="card">
			<h2>What leaves this browser</h2>
			<p class="faint">
				Only what Frog needs to name a title and log it: ids, season and episode numbers, a name and
				the page’s title, sent to your own Frog. Never an address, never your history. Nothing is
				counted or tracked.
			</p>
		</section>
	{/if}
</main>

<style>
	main {
		max-width: 680px;
		margin: 0 auto;
		padding: 32px 16px 60px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	header {
		display: flex;
		align-items: center;
		gap: 14px;
		margin-bottom: 6px;
	}

	header p {
		margin: 2px 0 0;
	}

	h2 {
		font-size: 1rem;
		margin-bottom: 8px;
	}

	.card p {
		margin: 0 0 10px;
	}

	.connect {
		display: grid;
		gap: 10px;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 6px;
		flex-wrap: wrap;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 14px;
		padding: 8px 0;
		cursor: pointer;
	}

	.row > span {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.row .faint {
		font-size: 12px;
	}

	.list {
		list-style: none;
		margin: 0 0 10px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.list li {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 8px 10px;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
	}

	.list:not(.rules) li {
		flex-direction: row;
		align-items: center;
		justify-content: space-between;
	}

	.rule-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		flex-wrap: wrap;
	}

	.rule-head > span:first-child {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.rule-head .faint {
		font-size: 12px;
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
	}

	.inline {
		display: flex;
		gap: 8px;
	}

	.code {
		font-family: ui-monospace, monospace;
		font-size: 12px;
	}

	.small {
		padding: 6px 10px;
		font-size: 12px;
	}

	.note,
	.problem {
		margin: 0;
		padding: 8px 12px;
		border-radius: var(--radius-sm);
		font-size: 13px;
	}

	.note {
		background: var(--accent-soft);
	}

	.problem {
		background: color-mix(in srgb, var(--bad) 16%, transparent);
		color: var(--bad);
	}

	.link {
		border: 0;
		background: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		font-weight: 650;
		cursor: pointer;
	}

	input[type='range'] {
		accent-color: var(--accent);
		width: 160px;
	}
</style>
