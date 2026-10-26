<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import { relativeDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { Account, DeviceSession } from '#lib/types';

	let account = $state<Account | null>(null);
	let devices = $state<DeviceSession[]>([]);

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

	async function toggleProfile(value: boolean) {
		try {
			const r = await api.setProfilePublic(value);
			if (account) account.profile_public = r.profile_public;
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

<div class="account">
	<div class="row card">
		<span class="who">
			<span class="avatar" aria-hidden="true"
				>{(account?.username ?? '?').slice(0, 1).toUpperCase()}</span
			>
			<span class="label">
				<strong>{account?.username ?? session.status?.username ?? ''}</strong>
				<span class="faint">
					{account ? ROLE[account.role] : ''}
					{#if account}· joined {relativeDate(account.created_at)}{/if}
				</span>
			</span>
		</span>
		<button class="btn ghost" onclick={switchAccount}>
			<Icon name="refresh" size={15} /> Switch account
		</button>
	</div>

	<div class="row card column">
		<div class="top">
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

	<div class="row card column">
		<div class="top">
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
				<li>
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

	<label class="row card">
		<span class="label">
			<strong>Show my profile</strong>
			<span class="faint">
				Others on this Frog can see what you watch, your favourites and your totals.
				{#if account}<a href="/people/{encodeURIComponent(account.username)}">See your profile</a
					>{/if}
			</span>
		</span>
		<input
			type="checkbox"
			checked={account?.profile_public ?? false}
			onchange={(e) => toggleProfile(e.currentTarget.checked)}
		/>
	</label>

	{#if account && account.role !== 'owner'}
		<div class="row card column">
			<div class="top">
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
	.account {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
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
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px;
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
		font-size: 0.82rem;
	}

	.label a {
		color: var(--accent);
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
		margin: 10px 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}

	.devices li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 10px 0;
		border-top: 1px solid var(--line);
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
