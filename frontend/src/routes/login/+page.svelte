<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';

	let username = $state('');
	let password = $state('');
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
				await api.login(username, password);
			}
			await session.refresh();
			await goto(next, { replaceState: true });
		} catch (e) {
			error = e instanceof Error ? e.message : 'Something went wrong';
		} finally {
			busy = false;
		}
	}
</script>

<div class="screen">
	<div class="glow" aria-hidden="true"></div>

	<form class="card panel" onsubmit={submit}>
		<div class="head">
			<span class="mark" aria-hidden="true"></span>
			<h1>{setup ? 'Set up Frog' : 'Frog'}</h1>
			<p class="muted">
				{setup
					? 'Pick the username and password you will use on this instance. This screen closes for good once the account exists.'
					: 'Your personal cinema.'}
			</p>
		</div>

		{#if session.offline}
			<p class="notice">Cannot reach the server. Check that it is running and you are on the same Tailnet.</p>
		{/if}

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

		{#if error}
			<p class="notice error" role="alert">{error}</p>
		{/if}

		<button class="btn primary wide" type="submit" disabled={busy}>
			{busy ? 'Working…' : setup ? 'Create account' : 'Sign in'}
		</button>

		{#if setup}
			<p class="faint small">
				Prefer not to type it here? Set <code>FROG_ADMIN_USERNAME</code> and
				<code>FROG_ADMIN_PASSWORD</code> in <code>.env</code> and restart.
			</p>
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
		width: 30px;
		height: 30px;
		border-radius: 9px;
		background: linear-gradient(140deg, var(--accent), color-mix(in oklab, var(--accent), #28e1a0));
		box-shadow: 0 0 26px -6px var(--accent);
		margin-bottom: 6px;
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

	.notice.error {
		border-color: color-mix(in srgb, var(--bad) 55%, transparent);
		background: color-mix(in srgb, var(--bad) 14%, var(--bg-elev-2));
	}

	.small {
		font-size: 0.74rem;
		line-height: 1.5;
	}

	code {
		font-family: var(--font-mono);
		font-size: 0.92em;
		background: var(--bg-elev-2);
		padding: 1px 5px;
		border-radius: 4px;
	}
</style>
