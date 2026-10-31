<script lang="ts">
	import { api, ApiError } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { relativeDate } from '#lib/format';
	import type {
		AdminUser,
		InstanceInfo,
		Invite,
		NewInvite,
		ServerEvent,
		ServerStatus
	} from '#lib/types';

	let invites = $state<Invite[]>([]);
	let loading = $state(true);
	let denied = $state(false);
	let busy = $state(false);

	let role = $state<'user' | 'admin'>('user');
	let days = $state('7');
	/** Shown once, right after it is made: the server keeps only a hash. */
	let fresh = $state<NewInvite | null>(null);

	const owner = $derived(session.status?.role === 'owner');

	let users = $state<AdminUser[]>([]);
	let instance = $state<InstanceInfo | null>(null);
	let server = $state<ServerStatus | null>(null);

	const ROLE: Record<AdminUser['role'], string> = {
		owner: 'Owner',
		admin: 'Administrator',
		user: 'Member'
	};

	async function loadPeople() {
		try {
			[users, instance, server] = await Promise.all([
				api.adminUsers().then((r) => r.users),
				api.instance(),
				api.server()
			]);
		} catch {
			// The invites request already reports a refusal.
		}
	}

	/** Whether this administrator may change that account at all. */
	function manageable(user: AdminUser): boolean {
		if (user.username === session.status?.username || user.role === 'owner') return false;
		return owner || user.role === 'user';
	}

	async function setRole(user: AdminUser, role: 'admin' | 'user') {
		try {
			const r = await api.updateUser(user.id, { role });
			user.role = r.role;
			toasts.push(
				role === 'admin' ? `${user.username} is an administrator` : `${user.username} is a member`
			);
		} catch (e) {
			toasts.error(e);
		}
	}

	async function setDisabled(user: AdminUser, disabled: boolean) {
		try {
			const r = await api.updateUser(user.id, { disabled });
			user.disabled = r.disabled;
			if (disabled) {
				toasts.undoable(`${user.username} is switched off`, async () => {
					const back = await api.updateUser(user.id, { disabled: false });
					user.disabled = back.disabled;
				});
			} else {
				toasts.push(`${user.username} can sign in again`);
			}
		} catch (e) {
			toasts.error(e);
		}
	}

	/**
	 * Deleting cannot be taken back once it happens, so it waits behind the
	 * toast's undo. The account is switched off at once, which signs it out,
	 * and only goes for good when the toast does.
	 */
	async function remove(user: AdminUser) {
		const wasOff = user.disabled;
		try {
			if (!wasOff) await api.updateUser(user.id, { disabled: true });
		} catch (e) {
			toasts.error(e);
			return;
		}
		users = users.filter((u) => u.id !== user.id);
		const timer = setTimeout(async () => {
			try {
				await api.deleteUser(user.id);
			} catch (e) {
				toasts.error(e);
				await loadPeople();
			}
		}, 7000);
		toasts.undoable(`${user.username} and everything they tracked will be deleted`, async () => {
			clearTimeout(timer);
			if (!wasOff) await api.updateUser(user.id, { disabled: false });
			await loadPeople();
		});
	}

	function size(bytes: number): string {
		if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
		if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
		return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
	}

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
		loadPeople();
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
			toasts.push('Invite withdrawn');
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
			toasts.push('Link copied');
		} catch {
			toasts.push('Could not copy. Select the link and copy it by hand', 'error');
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
	<header class="page-head">
		<div>
			<h1>Administration</h1>
			<p class="sub">This instance, and who can reach it.</p>
		</div>
	</header>

	{#if denied}
		<Empty
			art="none"
			title="This page is for administrators"
			hint="Ask whoever runs this instance if you need it."
		/>
	{:else}
		<div class="sections">
			<section class="block">
				<h2>People</h2>
				<ul class="users">
					{#each users as user (user.id)}
						<li class="card" class:off={user.disabled}>
							<div class="who">
								<span class="avatar" aria-hidden="true"
									>{user.username.slice(0, 1).toUpperCase()}</span
								>
								<span class="label">
									<strong>{user.username}</strong>
									<span class="faint">
										{ROLE[user.role]}
										· {user.titles === 1 ? '1 title' : `${user.titles} titles`}
										· {user.last_sign_in
											? `signed in ${relativeDate(user.last_sign_in)}`
											: 'not signed in yet'}
										{#if user.disabled}· <span class="off-label">switched off</span>{/if}
									</span>
								</span>
							</div>
							{#if manageable(user)}
								<div class="controls">
									{#if owner}
										<Select
											label="Role for {user.username}"
											compact
											value={user.role}
											options={[
												{ value: 'user', label: 'Member' },
												{ value: 'admin', label: 'Administrator' }
											]}
											onchange={(v) => setRole(user, v as 'admin' | 'user')}
										/>
									{/if}
									<button class="btn ghost" onclick={() => setDisabled(user, !user.disabled)}>
										{user.disabled ? 'Switch on' : 'Switch off'}
									</button>
									<button
										class="btn ghost danger"
										aria-label="Delete {user.username}"
										onclick={() => remove(user)}
									>
										<Icon name="trash" size={16} /> Delete
									</button>
								</div>
							{/if}
						</li>
					{/each}
				</ul>
			</section>

			<div class="col">
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
									{ value: 'user', label: 'Member' },
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
										<strong>{invite.role === 'admin' ? 'Administrator' : 'Member'}</strong>
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
			</div>
			<div class="col">
				<section class="block">
					<h2>This instance</h2>
					<dl class="facts card">
						<div>
							<dt>Version</dt>
							<dd>{session.status?.version ?? '—'}</dd>
						</div>
						<div>
							<dt>Accounts</dt>
							<dd>{instance?.accounts ?? '—'}</dd>
						</div>
						<div>
							<dt>Database</dt>
							<dd>{instance ? size(instance.database_bytes) : '—'}</dd>
						</div>
						<div>
							<dt>Artwork kept offline</dt>
							<dd>{instance ? size(instance.image_cache_bytes) : '—'}</dd>
						</div>
						<div>
							<dt>Updates</dt>
							<dd><a href="/settings#this-instance">Check in Settings</a></dd>
						</div>
					</dl>
				</section>

				<section class="block" id="server">
					<h2>Server</h2>
					<dl class="facts card">
						<div>
							<dt>Last backup</dt>
							<dd>
								{#if server?.backup}
									{@render outcome(server.backup)}
								{:else if instance?.last_backup}
									{relativeDate(instance.last_backup.at)}{instance.last_backup.pushed
										? ''
										: ' · kept on the server only'}
								{:else}
									None recorded yet
								{/if}
							</dd>
						</div>
						<div>
							<dt>Last update</dt>
							<dd>
								{#if server?.update}
									{@render outcome(server.update)}
								{:else}
									None recorded yet
								{/if}
							</dd>
						</div>
						{#if server?.rollback}
							<div>
								<dt>{server.rollback.kind === 'restore' ? 'Last restore' : 'Last rollback'}</dt>
								<dd>{@render outcome(server.rollback)}</dd>
							</div>
						{/if}
					</dl>
					<p class="faint">Administrators get a notification when one of these fails.</p>
				</section>
			</div>
		</div>
	{/if}
</div>

{#snippet outcome(event: ServerEvent)}
	<span class="outcome" class:failed={!event.ok}>
		{#if !event.ok}<Icon name="x" size={14} />{/if}
		<span>{event.reason}</span>
		<span class="faint">{relativeDate(event.at)}</span>
	</span>
{/snippet}

<style>
	#server dt {
		white-space: nowrap;
	}

	.outcome {
		display: inline-flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		align-items: center;
		gap: 4px 8px;
		text-align: right;
	}

	.outcome.failed {
		color: var(--bad);
	}

	/* People across the top; under it two steady columns, inviting on the
	   left and the instance on the right, each section starting on the same
	   line as its neighbour instead of wherever the grid left room. */
	.sections {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 40px 32px;
		align-items: start;
	}

	.sections > .block:first-child {
		grid-column: 1 / -1;
	}

	.col {
		display: grid;
		gap: 40px;
		min-width: 0;
	}

	@media (max-width: 980px) {
		.sections {
			grid-template-columns: minmax(0, 1fr);
		}
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
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
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
		background: var(--bg-elev-2);
	}

	.state.open {
		color: var(--good);
	}

	.state.expired {
		color: var(--text-faint);
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
		border-bottom: 1px solid var(--line);
	}

	.facts > div:last-child {
		border-bottom: 0;
	}

	.facts dt,
	.facts dd {
		margin: 0;
	}

	.users {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 10px;
	}

	.users li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px;
		padding: 12px 16px;
	}

	.users li.off {
		opacity: 0.7;
	}

	.who {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}

	.avatar {
		width: 36px;
		height: 36px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: var(--accent-soft);
		color: var(--accent);
		font-weight: 700;
		flex: none;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.8rem;
	}

	.off-label {
		color: var(--bad);
	}

	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.facts dd a {
		color: var(--accent);
	}

	.facts dt {
		color: var(--text-faint);
	}

	.danger {
		color: var(--bad);
	}
</style>
