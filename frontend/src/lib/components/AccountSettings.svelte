<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import { relativeDate } from '#lib/format';
	import { characters, nameLimit, USERNAME_MAX } from '#lib/names';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { Account, DeviceSession } from '#lib/types';

	let account = $state<Account | null>(null);
	let devices = $state<DeviceSession[]>([]);

	let renaming = $state(false);
	let newName = $state('');
	let renamePassword = $state('');

	let changing = $state(false);
	let current = $state('');
	let fresh = $state('');
	let busy = $state(false);

	let leaving = $state(false);
	let leavePassword = $state('');

	const ROLE: Record<Account['role'], string> = {
		owner: 'Owner',
		admin: 'Administrator',
		user: 'Member'
	};

	/** A phone or a computer, from the name the browser gave itself. */
	const onPhone = (device: DeviceSession) =>
		/android|iphone|ipad|mobile/i.test(device.device ?? '');

	async function load() {
		try {
			[account, devices] = await Promise.all([
				api.account(),
				api.sessions().then((r) => r.sessions)
			]);
		} catch (e) {
			toasts.error(e);
		}
	}
	load();

	function startRename() {
		// A name from before the limit could not be saved as it stands.
		const name = account?.username ?? '';
		newName = characters(name) <= USERNAME_MAX ? name : '';
		renaming = true;
	}

	async function rename(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		try {
			await api.changeUsername(newName, renamePassword);
			renamePassword = '';
			renaming = false;
			// Both: the session names the account everywhere, this row included.
			await session.refresh();
			account = await api.account();
			toasts.push('Username changed');
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function changePassword(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		try {
			const { signed_out } = await api.changePassword(current, fresh);
			current = '';
			fresh = '';
			changing = false;
			toasts.push(
				signed_out > 0
					? `Password changed. Signed out of ${signed_out === 1 ? '1 other device' : `${signed_out} other devices`}.`
					: 'Password changed'
			);
			devices = (await api.sessions()).sessions;
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function endSession(device: DeviceSession) {
		try {
			await api.endSession(device.id);
			devices = devices.filter((d) => d.id !== device.id);
			toasts.push('Signed out of that device');
		} catch (e) {
			toasts.error(e);
		}
	}

	async function endOthers() {
		try {
			const { signed_out } = await api.endOtherSessions();
			devices = devices.filter((d) => d.current);
			toasts.push(
				signed_out === 1 ? 'Signed out of 1 device' : `Signed out of ${signed_out} devices`
			);
		} catch (e) {
			toasts.error(e);
		}
	}

	async function switchAccount() {
		await session.logout();
		await goto('/login', { replace: true });
	}

	async function leave(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		try {
			await api.deleteAccount(leavePassword);
			await session.logout().catch(() => undefined);
			await goto('/login', { replace: true });
			toasts.push('Your account is gone');
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}
</script>

<div class="identity card">
	<Avatar
		src={session.status?.avatar ?? null}
		name={account?.username ?? session.status?.username ?? '?'}
		size={64}
	/>
	<span class="who">
		<strong class="name">{account?.username ?? session.status?.username ?? ''}</strong>
		<span class="meta">
			{#if account}
				<span class="role" data-role={account.role}
					>{#if account.role === 'owner'}<Icon name="crown" size={12} />{/if}{ROLE[
						account.role
					]}</span
				>
				<span class="faint">Joined {relativeDate(account.created_at)}</span>
			{/if}
		</span>
	</span>
	<button class="btn ghost" onclick={switchAccount}>
		<Icon name="refresh" size={15} /> Switch account
	</button>
</div>

<div class="account list-pane">
	<div class="row column">
		<div class="top">
			<span class="tile" aria-hidden="true"><Icon name="id" size={18} /></span>
			<span class="label">
				<strong>Username</strong>
				<span class="faint">It is how you sign in, and the address of your profile.</span>
			</span>
			{#if !renaming}
				<button class="btn" onclick={startRename}>Change</button>
			{/if}
		</div>
		{#if renaming}
			<form class="password" onsubmit={rename}>
				<label class="field">
					<span>New username</span>
					<input
						class="input"
						autocomplete="username"
						autocapitalize="none"
						spellcheck="false"
						minlength="2"
						maxlength={nameLimit(newName)}
						bind:value={newName}
						required
					/>
				</label>
				<label class="field">
					<span>Current password</span>
					<input
						class="input"
						type="password"
						autocomplete="current-password"
						bind:value={renamePassword}
						required
					/>
				</label>
				<div class="actions">
					<button type="button" class="btn ghost" onclick={() => (renaming = false)}>Cancel</button>
					<button class="btn primary" disabled={busy || !newName.trim() || !renamePassword}
						>Save username</button
					>
				</div>
			</form>
		{/if}
	</div>

	<div class="row column">
		<div class="top">
			<span class="tile" aria-hidden="true"><Icon name="key" size={18} /></span>
			<span class="label">
				<strong>Password</strong>
				<span class="faint">Changing it signs out your other devices.</span>
			</span>
			{#if !changing}
				<button class="btn" onclick={() => (changing = true)}>Change</button>
			{/if}
		</div>
		{#if changing}
			<form class="password" onsubmit={changePassword}>
				<label class="field">
					<span>Current password</span>
					<input
						class="input"
						type="password"
						autocomplete="current-password"
						bind:value={current}
						required
					/>
				</label>
				<label class="field">
					<span>New password</span>
					<input
						class="input"
						type="password"
						autocomplete="new-password"
						minlength="8"
						bind:value={fresh}
						required
					/>
				</label>
				<div class="actions">
					<button type="button" class="btn ghost" onclick={() => (changing = false)}>Cancel</button>
					<button class="btn primary" disabled={busy || fresh.length < 8 || !current}
						>Save password</button
					>
				</div>
			</form>
		{/if}
	</div>

	<div class="row column">
		<div class="top">
			<span class="tile" aria-hidden="true"><Icon name="shield" size={18} /></span>
			<span class="label">
				<strong>Signed-in devices</strong>
				<span class="faint"
					>{devices.length === 1 ? 'Just this one' : `${devices.length} devices`}</span
				>
			</span>
			{#if devices.length > 1}
				<button class="btn" onclick={endOthers}>Sign out everywhere else</button>
			{/if}
		</div>
		<ul class="devices">
			{#each devices as device (device.id)}
				<li class:current={device.current}>
					<span class="device-icon" aria-hidden="true"
						><Icon name={onPhone(device) ? 'phone' : 'desktop'} size={18} /></span
					>
					<span class="label">
						<strong>{device.device ?? 'A browser'}</strong>
						<span class="faint">
							{device.current ? 'This device' : `Signed in ${relativeDate(device.created_at)}`}
						</span>
					</span>
					{#if !device.current}
						<button
							class="btn ghost small"
							aria-label="Sign out {device.device ?? 'that browser'}"
							onclick={() => endSession(device)}>Sign out</button
						>
					{/if}
				</li>
			{/each}
		</ul>
	</div>

	{#if account && account.role !== 'owner'}
		<div class="row column">
			<div class="top">
				<span class="tile danger-tile" aria-hidden="true"><Icon name="trash" size={18} /></span>
				<span class="label">
					<strong>Delete account</strong>
					<span class="faint"
						>Your library, diary, shelves and settings go with it. This cannot be undone.</span
					>
				</span>
				{#if !leaving}
					<button class="btn ghost danger" onclick={() => (leaving = true)}>Delete…</button>
				{/if}
			</div>
			{#if leaving}
				<form class="password" onsubmit={leave}>
					<label class="field">
						<span>Your password, to confirm</span>
						<input
							class="input"
							type="password"
							autocomplete="current-password"
							bind:value={leavePassword}
							required
						/>
					</label>
					<div class="actions">
						<button type="button" class="btn ghost" onclick={() => (leaving = false)}
							>Keep my account</button
						>
						<button class="btn danger-fill" disabled={busy || !leavePassword}
							>Delete my account</button
						>
					</div>
				</form>
			{/if}
		</div>
	{/if}
</div>

<style>
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 15px 18px;
	}

	.row.column {
		flex-direction: column;
		flex-wrap: nowrap;
		align-items: stretch;
	}

	/* Who you are, as a card of its own over the settings. */
	.identity {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 16px;
		padding: 18px 20px;
		margin-bottom: 14px;
	}

	.who {
		flex: 1 1 12em;
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}

	.name {
		font-size: 1.3rem;
		font-weight: 750;
		letter-spacing: -0.01em;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		font-size: 0.82rem;
	}

	.role {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 9px;
		border-radius: var(--radius-pill);
		background: var(--accent-soft);
		color: var(--accent);
		font-weight: 650;
		font-size: 0.76rem;
	}

	.top {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 12px;
	}

	.top .label {
		flex: 1 1 10em;
	}

	/* Each setting leads with its icon, as a phone's settings do. */
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

	.danger-tile {
		background: color-mix(in srgb, var(--bad) 14%, transparent);
		color: var(--bad);
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

	.password {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 220px), 1fr));
		gap: 12px;
		margin-top: 12px;
	}

	.actions {
		grid-column: 1 / -1;
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}

	.devices {
		list-style: none;
		margin: 12px 0 0 48px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.devices li {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 12px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		border: 1px solid var(--line);
	}

	.devices li .label {
		flex: 1;
	}

	.devices li.current {
		border-color: color-mix(in srgb, var(--accent) 40%, var(--line));
	}

	.device-icon {
		flex: none;
		color: var(--text-dim);
	}

	@media (max-width: 600px) {
		.devices {
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

	.danger-fill {
		background: var(--bad);
		border-color: transparent;
		color: #fff;
	}
</style>
