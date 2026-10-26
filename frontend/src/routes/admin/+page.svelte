<script lang="ts">
	import { api, ApiError } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { Invite, NewInvite } from '#lib/types';

	let invites = $state<Invite[]>([]);
	let loading = $state(true);
	let denied = $state(false);
	let busy = $state(false);

	let role = $state<'user' | 'admin'>('user');
	let days = $state('7');
	/** Shown once, right after it is made: the server keeps only a hash. */
	let fresh = $state<NewInvite | null>(null);

	const owner = $derived(session.status?.role === 'owner');

	async function load() {
		loading = true;
		try {
			invites = (await api.invites()).invites;
			denied = false;
		} catch (e) {
			denied = e instanceof ApiError && e.status === 403;
			if (!denied) toasts.push(e instanceof Error ? e.message : 'Could not load invites');
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		load();
	});

	async function create() {
		busy = true;
		try {
			fresh = await api.createInvite(role, Number(days));
			await load();
		} catch (e) {
			toasts.push(e instanceof Error ? e.message : 'Could not make an invite');
		} finally {
			busy = false;
		}
	}

	async function revoke(invite: Invite) {
		try {
			await api.revokeInvite(invite.id);
			if (fresh && invite.state === 'open') fresh = null;
			await load();
			toasts.push('Invite withdrawn.');
		} catch (e) {
			toasts.push(e instanceof Error ? e.message : 'Could not withdraw it');
		}
	}

	function linkFor(invite: NewInvite): string {
		return `${location.origin}${invite.path}`;
	}

	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			toasts.push('Link copied.');
		} catch {
			toasts.push('Could not copy. Select the link and copy it by hand.');
		}
	}

	function when(iso: string): string {
		return new Date(iso).toLocaleDateString(undefined, {
			day: 'numeric',
			month: 'short',
			year: 'numeric'
		});
	}
</script>

<svelte:head>
	<title>Administration · Frog</title>
</svelte:head>

<div class="page">
	<header>
		<h1>Administration</h1>
		<p class="muted">This instance, and who can reach it.</p>
	</header>

	{#if denied}
		<Empty
			art="none"
			title="This page is for administrators"
			hint="Ask whoever runs this instance if you need it."
		/>
	{:else}
		<section class="block">
			<h2>Invite someone</h2>
			<p class="muted">
				A link that works once and then stops. Nobody can create an account without one.
			</p>

			<div class="row">
				<label class="field">
					<span>Role</span>
					<Select
						value={role}
						label="Role"
						onchange={(v) => (role = v as 'user' | 'admin')}
						options={[
							{ value: 'user', label: 'User' },
							...(owner ? [{ value: 'admin', label: 'Administrator' }] : [])
						]}
					/>
				</label>

				<label class="field">
					<span>Expires after</span>
					<Select
						value={days}
						label="Expires after"
						onchange={(v) => (days = v)}
						options={[
							{ value: '1', label: 'A day' },
							{ value: '7', label: 'A week' },
							{ value: '30', label: 'A month' }
						]}
					/>
				</label>

				<button class="btn primary" onclick={create} disabled={busy}>
					{busy ? 'Making it…' : 'Make a link'}
				</button>
			</div>

			{#if fresh}
				<div class="fresh card">
					<p>
						<strong>Copy this now.</strong> It is shown once — Frog keeps only a fingerprint of it.
					</p>
					<code>{linkFor(fresh)}</code>
					<button class="btn" onclick={() => copy(linkFor(fresh!))}>
						<Icon name="copy" size={15} />
						Copy link
					</button>
				</div>
			{/if}
		</section>

		<section class="block">
			<h2>Invites</h2>
			{#if loading}
				<p class="muted">Loading…</p>
			{:else if invites.length === 0}
				<Empty art="none" title="No invites yet" hint="Make one above to let somebody in." />
			{:else}
				<ul class="invites">
					{#each invites as invite (invite.id)}
						<li class="card">
							<div>
								<strong>{invite.role === 'admin' ? 'Administrator' : 'User'}</strong>
								<span class="state {invite.state}">{invite.state}</span>
							</div>
							<p class="muted">
								{#if invite.state === 'used'}
									Used by {invite.used_by ?? 'someone'} on {when(invite.used_at ?? '')}
								{:else if invite.state === 'expired'}
									Expired on {when(invite.expires_at)}
								{:else}
									Expires on {when(invite.expires_at)}
								{/if}
								{#if invite.created_by}· made by {invite.created_by}{/if}
							</p>
							{#if invite.state === 'open'}
								<button class="btn ghost" onclick={() => revoke(invite)}>Withdraw</button>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section class="block">
			<h2>This instance</h2>
			<dl class="facts card">
				<div>
					<dt>Version</dt>
					<dd>{session.status?.version ?? '—'}</dd>
				</div>
				<div>
					<dt>Signed in as</dt>
					<dd>{session.status?.username ?? '—'}</dd>
				</div>
				<div>
					<dt>Your role</dt>
					<dd>{session.status?.role ?? '—'}</dd>
				</div>
			</dl>
		</section>
	{/if}
</div>

<style>
	.page {
		max-width: 760px;
		margin: 0 auto;
		padding: 24px 16px 64px;
		display: grid;
		gap: 28px;
	}

	header h1 {
		margin: 0;
		font-size: 1.6rem;
	}

	header p {
		margin: 4px 0 0;
	}

	.block {
		display: grid;
		gap: 12px;
	}

	.block h2 {
		margin: 0;
		font-size: 1.05rem;
	}

	.block > p {
		margin: 0;
		font-size: 0.9rem;
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 12px;
		align-items: end;
	}

	.fresh {
		display: grid;
		gap: 10px;
		padding: 16px;
	}

	.fresh p {
		margin: 0;
		font-size: 0.9rem;
	}

	.fresh code {
		word-break: break-all;
		font-size: 0.85rem;
		padding: 10px;
		border-radius: var(--radius-sm, 8px);
		background: var(--surface-2, rgba(255, 255, 255, 0.05));
	}

	.fresh .btn {
		justify-self: start;
	}

	.invites {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 10px;
	}

	.invites li {
		padding: 14px 16px;
		display: grid;
		gap: 4px;
	}

	.invites li > div {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.invites p {
		margin: 0;
		font-size: 0.85rem;
	}

	.invites .btn {
		justify-self: start;
		margin-top: 6px;
	}

	.state {
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 2px 8px;
		border-radius: 999px;
		background: var(--surface-2, rgba(255, 255, 255, 0.07));
	}

	.state.open {
		color: var(--ok, #30a46c);
	}

	.state.expired {
		color: var(--muted, #8b8b8b);
	}

	.facts {
		display: grid;
		gap: 0;
		margin: 0;
		padding: 4px 16px;
	}

	.facts > div {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		padding: 10px 0;
		border-bottom: 1px solid var(--line, rgba(255, 255, 255, 0.08));
	}

	.facts > div:last-child {
		border-bottom: 0;
	}

	.facts dt,
	.facts dd {
		margin: 0;
	}

	.facts dt {
		color: var(--muted, #8b8b8b);
	}
</style>
