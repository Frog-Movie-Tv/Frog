<script lang="ts">
	import { onDestroy } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { api } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import { relativeDate } from '#lib/format';
	import { toasts } from '#lib/toast.svelte';
	import type { ApiToken } from '#lib/types';

	/** Long enough for the toast's Undo, which lasts 7 s. */
	const REVOKE_AFTER = 7500;

	let tokens = $state<ApiToken[]>([]);
	let naming = $state(false);
	let name = $state('');
	let busy = $state(false);
	/** The token just made, shown until this card is left. */
	let fresh = $state<{ id: number; secret: string } | null>(null);

	/** Revokes waiting out their Undo, by token. */
	const pending = new SvelteMap<number, ReturnType<typeof setTimeout>>();

	async function load() {
		try {
			tokens = (await api.tokens()).tokens;
		} catch (e) {
			toasts.error(e);
		}
	}
	load();

	async function create(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		try {
			const made = await api.createToken(name);
			fresh = { id: made.token.id, secret: made.secret };
			tokens = [made.token, ...tokens];
			naming = false;
			name = '';
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function copy() {
		if (!fresh) return;
		try {
			await navigator.clipboard.writeText(fresh.secret);
			toasts.push('Token copied');
		} catch {
			toasts.push('Select the token and copy it', 'error');
		}
	}

	function sendRevoke(id: number) {
		pending.delete(id);
		api.revokeToken(id).catch((e) => {
			toasts.error(e);
			load();
		});
	}

	function revoke(token: ApiToken) {
		tokens = tokens.filter((t) => t.id !== token.id);
		if (fresh?.id === token.id) fresh = null;
		pending.set(
			token.id,
			setTimeout(() => sendRevoke(token.id), REVOKE_AFTER)
		);
		toasts.undoable(`${token.name} revoked`, () => {
			const timer = pending.get(token.id);
			if (timer === undefined) return;
			clearTimeout(timer);
			pending.delete(token.id);
			tokens = [...tokens, token].sort((a, b) => b.created_at.localeCompare(a.created_at));
		});
	}

	/** A revoke never waits on a page that is going away: it goes now. */
	function flush() {
		for (const [id, timer] of pending) {
			clearTimeout(timer);
			pending.delete(id);
			void fetch(`/api/account/tokens/${id}`, {
				method: 'DELETE',
				credentials: 'same-origin',
				keepalive: true
			});
		}
	}

	$effect(() => {
		window.addEventListener('pagehide', flush);
		return () => window.removeEventListener('pagehide', flush);
	});
	onDestroy(flush);
</script>

<div class="extension list-pane">
	<div class="row column">
		<div class="top">
			<span class="tile" aria-hidden="true"><Icon name="key" size={18} /></span>
			<span class="label">
				<strong>Browser extension</strong>
				<span class="faint"
					>Logs what you watch on any site. A token lets it read your library and log watches,
					nothing else.</span
				>
			</span>
			{#if !naming}
				<button class="btn" onclick={() => (naming = true)}>New token</button>
			{/if}
		</div>

		{#if naming}
			<form class="new" onsubmit={create}>
				<label class="field">
					<span>Name</span>
					<input
						class="input"
						bind:value={name}
						maxlength="40"
						placeholder="Brave on the laptop"
						required
					/>
				</label>
				<div class="actions">
					<button type="button" class="btn ghost" onclick={() => (naming = false)}>Cancel</button>
					<button class="btn primary" disabled={busy || !name.trim()}>Create token</button>
				</div>
			</form>
		{/if}

		{#if fresh}
			<div class="fresh" role="status">
				<span class="faint">Copy it into the extension now. Frog shows it only once.</span>
				<div class="secret">
					<code data-testid="token-secret">{fresh.secret}</code>
					<button class="btn small" onclick={copy}><Icon name="copy" size={15} /> Copy</button>
				</div>
			</div>
		{/if}

		{#if tokens.length > 0}
			<ul class="tokens">
				{#each tokens as token (token.id)}
					<li>
						<span class="label">
							<strong>{token.name}</strong>
							<span class="faint">
								{token.last_used_at
									? `Last used ${relativeDate(token.last_used_at)}`
									: `Made ${relativeDate(token.created_at)}, not used yet`}
							</span>
						</span>
						<button
							class="btn ghost small danger"
							aria-label="Revoke {token.name}"
							onclick={() => revoke(token)}>Revoke</button
						>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</div>

<style>
	.extension {
		margin-top: 14px;
	}

	.row {
		display: flex;
		gap: 16px;
		padding: 15px 18px;
	}

	.row.column {
		flex-direction: column;
		align-items: stretch;
	}

	.top {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 12px;
	}

	.top .label {
		flex: 1 1 12em;
	}

	.tile {
		flex: none;
		width: 36px;
		height: 36px;
		display: grid;
		place-items: center;
		border-radius: 10px;
		background: var(--accent-soft);
		color: var(--accent);
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.82rem;
	}

	.new {
		display: grid;
		gap: 12px;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}

	.fresh {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border-radius: var(--radius-sm);
		border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--line));
		background: var(--bg-elev-2);
		font-size: 0.86rem;
	}

	.secret {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
	}

	.secret code {
		flex: 1;
		min-width: 0;
		padding: 8px 10px;
		border-radius: var(--radius-sm);
		background: var(--bg);
		font-size: 0.8rem;
		user-select: all;
		overflow-wrap: anywhere;
	}

	.tokens {
		list-style: none;
		margin: 0 0 0 48px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.tokens li {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 12px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		border: 1px solid var(--line);
	}

	.tokens li .label {
		flex: 1;
	}

	@media (max-width: 600px) {
		.tokens {
			margin-left: 0;
		}
	}

	.small {
		padding: 6px 12px;
		font-size: 0.82rem;
	}

	.danger {
		color: var(--bad);
	}
</style>
