<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '#lib/api';
	import Logo from '#lib/components/Logo.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';

	let username = $state('');
	let password = $state('');
	let remember = $state(true);
	let code = $state('');
	let askCode = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);

	const setup = $derived(session.needsSetup);
	const next = $derived(page.url.searchParams.get('next') ?? '/');

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		try {
			if (setup) {
				await api.setup(username, password);
				toasts.push('Account created. Welcome to Frog.');
			} else {
				await api.login(username, password, remember, askCode ? code : undefined);
			}
			await session.refresh();
			await goto(next, { replaceState: true });
		} catch (e) {
			if (e instanceof ApiError && e.twoFactor) {
				// The first ask is a step, not an error.
				error = askCode ? e.message : null;
				askCode = true;
				code = '';
			} else {
				error = e instanceof Error ? e.message : 'Something went wrong';
			}
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>{setup ? 'Set up' : 'Sign in'} · Frog</title>
</svelte:head>

<div class="screen">
	<div class="glow" aria-hidden="true"></div>

	<form class="card panel" onsubmit={submit}>
		<div class="head">
			<span class="mark" aria-hidden="true"><Logo size={44} /></span>
			<h1>{setup ? 'Set up Frog' : 'Frog'}</h1>
			<p class="muted">
				{setup
					? 'Choose the account you will sign in with.'
					: askCode
						? 'Enter the code from your authenticator app.'
						: 'Your film and television diary.'}
			</p>
		</div>

		{#if session.offline}
			<p class="notice">Cannot reach the server.</p>
		{/if}

		{#if askCode}
			<label class="field">
				<span>Code</span>
				<!-- svelte-ignore a11y_autofocus -->
				<input
					class="input code"
					bind:value={code}
					autocomplete="one-time-code"
					inputmode="text"
					autocapitalize="off"
					spellcheck="false"
					placeholder="123 456"
					required
					autofocus
				/>
				<small class="faint">Lost your phone? Use one of your recovery codes.</small>
			</label>
		{/if}

		<div class="credentials" hidden={askCode}>
			<label class="field">
				<span>Username</span>
				<input
					class="input"
					bind:value={username}
					autocomplete="username"
					required
					autocapitalize="off"
					spellcheck="false"
				/>
			</label>

			<label class="field">
				<span>Password</span>
				<input
					class="input"
					type="password"
					bind:value={password}
					autocomplete={setup ? 'new-password' : 'current-password'}
					required
					minlength={setup ? 8 : undefined}
				/>
			</label>

			{#if !setup}
				<label class="remember">
					<input type="checkbox" bind:checked={remember} />
					<span>Keep me signed in</span>
				</label>
			{/if}
		</div>

		{#if error}
			<p class="notice error" role="alert">{error}</p>
		{/if}

		<button class="btn primary wide" type="submit" disabled={busy}>
			{busy ? 'Working…' : setup ? 'Create account' : askCode ? 'Verify' : 'Sign in'}
		</button>

		{#if askCode}
			<button
				class="btn ghost wide"
				type="button"
				onclick={() => {
					askCode = false;
					error = null;
				}}>Back</button
			>
		{/if}
	</form>
</div>

<style>
	.screen {
		min-height: 100dvh;
		display: grid;
		place-items: center;
		padding: 24px;
		position: relative;
		overflow: hidden;
	}

	/* A single soft light source behind the card: cheap, and it stops the
	   sign-in screen looking like a bare form on black. */
	.glow {
		position: absolute;
		width: min(90vw, 760px);
		aspect-ratio: 1;
		border-radius: 50%;
		background: radial-gradient(
			circle,
			color-mix(in srgb, var(--accent) 26%, transparent),
			transparent 62%
		);
		filter: blur(30px);
		pointer-events: none;
	}

	.panel {
		position: relative;
		width: min(100%, 390px);
		padding: 30px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		box-shadow: var(--shadow-3);
		background: color-mix(in srgb, var(--bg-elev) 88%, transparent);
		backdrop-filter: blur(14px);
	}

	.head {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-bottom: 4px;
	}

	.mark {
		display: block;
		margin-bottom: 6px;
		filter: drop-shadow(0 6px 18px color-mix(in srgb, var(--accent) 35%, transparent));
	}

	h1 {
		font-size: 1.6rem;
	}

	.head p {
		font-size: 0.86rem;
	}

	.wide {
		width: 100%;
		padding: 12px;
		margin-top: 4px;
	}

	.notice {
		padding: 10px 13px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		border: 1px solid var(--line-strong);
		font-size: 0.84rem;
	}

	.credentials {
		display: contents;
	}

	.credentials[hidden] {
		display: none;
	}

	.code {
		font-size: 1.25rem;
		letter-spacing: 0.12em;
		text-align: center;
		font-variant-numeric: tabular-nums;
	}

	.remember {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.86rem;
		color: var(--text-dim);
	}

	.notice.error {
		border-color: color-mix(in srgb, var(--bad) 55%, transparent);
		background: color-mix(in srgb, var(--bad) 14%, var(--bg-elev-2));
	}
</style>
