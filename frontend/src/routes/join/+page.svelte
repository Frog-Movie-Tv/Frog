<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '#lib/api';
	import Logo from '#lib/components/Logo.svelte';
	import { session } from '#lib/session.svelte';

	let username = $state('');
	let password = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);

	const invite = $derived(page.url.searchParams.get('code') ?? '');

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		try {
			await api.register(username, password, invite);
			await session.refresh();
			// Same as first-run setup: a new account goes to the welcome
			// rather than flashing an empty Home on the way there.
			await goto('/welcome', { replaceState: true }).catch(() => {});
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Something went wrong';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Join · Frog</title>
</svelte:head>

<div class="screen">
	<div class="glow" aria-hidden="true"></div>

	<form class="card panel" onsubmit={submit}>
		<div class="head">
			<span class="mark" aria-hidden="true"><Logo size={44} /></span>
			<h1>Join Frog</h1>
			<p class="muted">
				{invite
					? 'Pick a username and a password. Your library is yours alone.'
					: 'This page needs an invite link.'}
			</p>
		</div>

		{#if invite}
			<label class="field">
				<span>Username</span>
				<input
					class="input"
					bind:value={username}
					autocomplete="username"
					autocapitalize="none"
					spellcheck="false"
					required
					minlength="2"
				/>
			</label>

			<label class="field">
				<span>Password</span>
				<input
					class="input"
					type="password"
					bind:value={password}
					autocomplete="new-password"
					required
					minlength="8"
				/>
				<small class="faint">At least 8 characters.</small>
			</label>

			{#if error}
				<p class="notice error" role="alert">{error}</p>
			{/if}

			<button class="btn primary wide" type="submit" disabled={busy}>
				{busy ? 'Creating your account…' : 'Create my account'}
			</button>
		{:else}
			<p class="notice">
				An invite link looks like <code>/join?code=…</code>. Ask whoever runs this Frog for one.
			</p>
			<a class="btn wide" href="/login">Sign in instead</a>
		{/if}

		{#if invite}
			<p class="foot muted">Already have an account? <a href="/login">Sign in</a></p>
		{/if}
	</form>
</div>

<style>
	/* The same card as signing in: it is the first thing an invited person
	   sees, so it should look like the place they are joining. */
	.screen {
		min-height: 100dvh;
		display: grid;
		place-items: center;
		padding: 24px;
		position: relative;
		overflow: hidden;
	}

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

	.field small {
		font-size: 0.78rem;
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

	.foot {
		text-align: center;
		font-size: 0.85rem;
	}

	.foot a {
		color: var(--accent);
	}
</style>
