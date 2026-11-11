<script lang="ts">
	import { api, ApiError } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { relativeDate, fileSize } from '#lib/format';
	import type {
		AdminUser,
		InstanceInfo,
		Invite,
		NewInvite,
		ResetLink,
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

	/** The same: a reset link is shown once, with who it is for. */
	let reset = $state<{ username: string; link: ResetLink } | null>(null);

	const owner = $derived(session.status?.role === 'owner');

	let users = $state<AdminUser[]>([]);
	let instance = $state<InstanceInfo | null>(null);
	let server = $state<ServerStatus | null>(null);

	const ROLE: Record<AdminUser['role'], string> = {
		owner: 'Owner',
		admin: 'Administrator',
		user: 'Member'
	};

	/** "1 owner · 2 members": who is here, by role. */
	const roleCounts = $derived.by(() => {
		const parts: string[] = [];
		for (const [key, one, many] of [
			['owner', 'owner', 'owners'],
			['admin', 'administrator', 'administrators'],
			['user', 'member', 'members']
		] as const) {
			const n = users.filter((u) => u.role === key).length;
			if (n) parts.push(`${n} ${n === 1 ? one : many}`);
		}
		return parts.join(' · ');
	});

	const openInvites = $derived(invites.filter((i) => i.state === 'open'));
	const pastInvites = $derived(invites.filter((i) => i.state !== 'open'));

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

	/** A change to an account withdraws its link, so stop offering it. */
	function dropReset(user: AdminUser) {
		if (reset?.username === user.username) reset = null;
	}

	async function setRole(user: AdminUser, role: 'admin' | 'user') {
		try {
			const r = await api.updateUser(user.id, { role });
			user.role = r.role;
			dropReset(user);
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
			dropReset(user);
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
		dropReset(user);
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

	const size = fileSize;

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

	function linkFor(invite: { path: string }): string {
		return `${location.origin}${invite.path}`;
	}

	async function makeReset(user: AdminUser) {
		try {
			reset = { username: user.username, link: await api.createResetLink(user.id) };
		} catch (e) {
			toasts.error(e);
		}
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
		{#if !denied}
			<div class="actions">
				<a class="btn" href="/admin/database"><Icon name="database" size={16} /> Database</a>
			</div>
		{/if}
	</header>

	{#if denied}
		<Empty
			art="none"
			title="This page is for administrators"
			hint="Ask whoever runs this instance if you need it."
		/>
	{:else}
		<!-- The instance at a glance; the detail is further down. -->
		<div class="overview">
			<div class="kpi card">
				<span class="kpi-icon" aria-hidden="true"><Icon name="users" size={18} /></span>
				<span class="eyebrow">People</span>
				<strong>{users.length || '—'}</strong>
				<span class="faint">{roleCounts}</span>
			</div>
			<div class="kpi card">
				<span class="kpi-icon" aria-hidden="true"><Icon name="film" size={18} /></span>
				<span class="eyebrow">Titles known</span>
				<strong>{instance?.titles?.all.toLocaleString() ?? '—'}</strong>
				{#if instance?.titles}
					<span class="faint"
						>{instance.titles.films.toLocaleString()} films · {instance.titles.shows.toLocaleString()}
						shows{#if instance.titles.manga}&nbsp;· {instance.titles.manga.toLocaleString()} manga{/if}</span
					>
				{/if}
			</div>
			<div class="kpi card">
				<span class="kpi-icon" aria-hidden="true"><Icon name="database" size={18} /></span>
				<span class="eyebrow">Storage</span>
				<strong>{instance ? size(instance.database_bytes) : '—'}</strong>
				{#if instance}<span class="faint">and {size(instance.image_cache_bytes)} of artwork</span
					>{/if}
			</div>
			<a class="kpi card" href="#server">
				<span class="kpi-icon" aria-hidden="true"><Icon name="server" size={18} /></span>
				<span class="eyebrow">Server</span>
				<strong class="health">
					<span
						class="dot"
						class:bad={server?.backup?.ok === false || server?.update?.ok === false}
						aria-hidden="true"
					></span>
					{server?.backup?.ok === false || server?.update?.ok === false
						? 'Needs a look'
						: 'Healthy'}
				</strong>
				<span class="faint">Version {session.status?.version ?? '—'}</span>
			</a>
		</div>

		<div class="sections">
			<section class="block">
				<div class="block-head">
					<h2>People</h2>
					<a class="btn ghost small" href="#invite"><Icon name="user-plus" size={15} /> Invite</a>
				</div>
				<ul class="users list-pane">
					{#each users as user (user.id)}
						<li class:off={user.disabled}>
							<div class="who">
								<span class="avatar" data-role={user.role} aria-hidden="true"
									>{user.username.slice(0, 1).toUpperCase()}</span
								>
								<span class="label">
									<span class="name-line">
										<strong>{user.username}</strong>
										<span class="role" data-role={user.role}
											>{#if user.role === 'owner'}<Icon name="crown" size={11} />{/if}{ROLE[
												user.role
											]}</span
										>
										{#if user.disabled}<span class="role off-label">Switched off</span>{/if}
									</span>
									<span class="faint">
										{user.titles === 1 ? '1 title' : `${user.titles.toLocaleString()} titles`}
										· {user.last_sign_in
											? `signed in ${relativeDate(user.last_sign_in)}`
											: 'not signed in yet'}
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
									{#if !user.disabled}
										<button
											class="btn ghost"
											aria-label="Reset link for {user.username}"
											onclick={() => makeReset(user)}
										>
											Reset link
										</button>
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

				{#if reset}
					<div class="fresh card reset">
						<p>
							<strong>Copy this now.</strong> Give it to {reset.username}. It sets a new password
							once, works for a day, and is shown only here.
						</p>
						<code>{linkFor(reset.link)}</code>
						<button class="btn" onclick={() => reset && copy(linkFor(reset.link))}>
							<Icon name="copy" size={15} />
							Copy link
						</button>
					</div>
				{/if}
			</section>

			<div class="col">
				<section class="block" id="invite">
					<h2>Invite someone</h2>
					<div class="invite card">
						<div class="invite-head">
							<span class="kpi-icon" aria-hidden="true"><Icon name="ticket" size={18} /></span>
							<p class="muted">
								A link that works once and then stops. Nobody can create an account without one.
							</p>
						</div>
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
						{#if openInvites.length}
							<ul class="invites list-pane">
								{#each openInvites as invite (invite.id)}
									{@render inviteRow(invite)}
								{/each}
							</ul>
						{:else}
							<p class="faint">No open invites.</p>
						{/if}
						{#if pastInvites.length}
							<details class="past">
								<summary>
									<Icon name="chevron-right" size={14} />
									Used and expired <span class="faint">{pastInvites.length}</span>
								</summary>
								<ul class="invites list-pane">
									{#each pastInvites as invite (invite.id)}
										{@render inviteRow(invite)}
									{/each}
								</ul>
							</details>
						{/if}
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
							<dt>Titles known</dt>
							<dd>{instance?.titles?.all.toLocaleString() ?? '—'}</dd>
						</div>
						{#if instance?.titles}
							{@const known = instance.titles}
							{#each [['Films', known.films], ['Shows', known.shows], ['Manga', known.manga ?? 0]] as [name, n] (name)}
								<div class="part">
									<dt>{name}</dt>
									<dd>{Number(n).toLocaleString()}</dd>
								</div>
							{/each}
						{/if}
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

{#snippet inviteRow(invite: Invite)}
	<li>
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
{/snippet}

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

	.overview {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 200px), 1fr));
		gap: 12px;
		margin-bottom: 36px;
	}

	.kpi {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 16px 18px;
		color: inherit;
		text-decoration: none;
		min-width: 0;
	}

	a.kpi:hover {
		background: var(--bg-hover);
	}

	.kpi strong {
		font-size: 1.5rem;
		font-weight: 750;
		letter-spacing: -0.01em;
		font-variant-numeric: tabular-nums;
	}

	.kpi .faint {
		font-size: 0.78rem;
	}

	.kpi-icon {
		flex: none;
		width: 34px;
		height: 34px;
		display: grid;
		place-items: center;
		margin-bottom: 6px;
		border-radius: 10px;
		background: var(--accent-soft);
		color: var(--accent);
	}

	.health {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 1.15rem !important;
	}

	.dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--good);
		flex: none;
	}

	.dot.bad {
		background: var(--bad);
	}

	.block-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.small {
		min-height: 32px;
		padding: 0 12px;
		font-size: 0.8rem;
	}

	.name-line {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.role {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 1px 8px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-size: 0.72rem;
		font-weight: 650;
	}

	.role[data-role='owner'],
	.role[data-role='admin'] {
		background: var(--accent-soft);
		color: var(--accent);
	}

	.role.off-label {
		background: color-mix(in srgb, var(--bad) 14%, transparent);
	}

	.avatar[data-role='owner'] {
		box-shadow: 0 0 0 2px var(--accent);
	}

	.invite {
		display: grid;
		gap: 4px;
		padding: 16px 0 0;
	}

	.invite-head {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 0 18px;
	}

	.invite-head .kpi-icon {
		margin: 0;
	}

	.invite-head p {
		margin: 0;
		font-size: 0.88rem;
	}

	.past summary {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 4px 10px;
		font-size: 0.88rem;
		font-weight: 600;
		color: var(--text-dim);
		cursor: pointer;
		list-style: none;
	}

	.past summary::-webkit-details-marker {
		display: none;
	}

	.past[open] summary :global(svg) {
		transform: rotate(90deg);
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

	/* A quiet label over each group, the way a phone's settings name theirs. */
	.block h2 {
		margin: 0;
		padding-left: 4px;
		font-size: 0.95rem;
		font-weight: 650;
		letter-spacing: normal;
		color: var(--text-dim);
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
		padding: 16px 18px;
	}

	.fresh {
		display: grid;
		gap: 10px;
		padding: 16px;
	}

	.fresh.reset {
		margin-top: 12px;
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
	}

	.invites li {
		padding: 14px 18px;
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
		display: inline-block;
		font-size: 0.78rem;
		padding: 2px 8px;
		border-radius: 999px;
		background: var(--bg-elev-2);
	}

	.state::first-letter {
		text-transform: uppercase;
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

	/* What a total is made of, each on a row of its own under it. */
	.facts > div.part {
		padding: 6px 0 6px 16px;
		font-size: 0.9rem;
	}

	.facts dt,
	.facts dd {
		margin: 0;
	}

	.users {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.users li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px;
		padding: 14px 18px;
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
		width: 40px;
		height: 40px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
		color: var(--text);
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

	/* A person's controls: small and quiet; deleting is red words, not a box. */
	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}

	.controls .btn {
		min-height: 32px;
		padding: 0 13px;
		font-size: 0.8rem;
		border-color: transparent;
		background: var(--bg-elev-2);
	}

	.controls .btn.danger {
		background: transparent;
		box-shadow: none;
	}

	.controls .btn.danger:hover {
		background: color-mix(in srgb, var(--bad) 14%, transparent);
	}

	/* Still a finger's width on a phone. */
	@media (max-width: 760px) {
		.controls .btn {
			min-height: 44px;
		}
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
