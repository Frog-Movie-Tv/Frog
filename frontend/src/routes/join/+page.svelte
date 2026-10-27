<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '#lib/api';
	import Logo from '#lib/components/Logo.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';

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
			toasts.push(`Welcome to Frog, ${username}.`);
			// Same as first-run setup: a new account goes to the welcome
			// rather than flashing an empty Home on the way there.
			await goto('/welcome', { replaceState: true });
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
					? 'Pick a name and a password. Your library is yours alone.'
					: 'This page needs an invite link.'}
			</p>
		</div>

		{#if invite}
			<label class="field">
				<span>Name</span>
				<input
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
					type="password"
					bind:value={password}
					autocomplete="new-password"
					required
					minlength="8"
				/>
				<small class="muted">At least 8 characters.</small>
			</label>

			{#if error}
				<p class="error" role="alert">{error}</p>
			{/if}

			<button class="btn primary" type="submit" disabled={busy}>
				{busy ? 'Creating your account…' : 'Create my account'}
			</button>
		{:else}
			<p class="error" role="alert">
				An invite link looks like <code>/join?code=…</code>. Ask whoever runs this instance for one.
			</p>
			<a class="btn" href="/login">Sign in instead</a>
		{/if}

		<p class="foot muted">
			Already have an account? <a href="/login">Sign in</a>.
		</p>
	</form>
</div>

<style>
	.screen {
		min-height: 100dvh;
		display: grid;
		place-items: center;
		padding: 24px 16px;
		position: relative;
	}

	.glow {
		position: absolute;
		inset: 0;
		background: radial-gradient(
			60% 40% at 50% 0%,
			var(--accent-tint, transparent),
			transparent 70%
		);
		pointer-events: none;
	}

	.card {
		width: min(400px, 100%);
		display: grid;
		gap: 16px;
		padding: 28px;
		position: relative;
	}

	.head {
		display: grid;
		justify-items: center;
		gap: 6px;
		text-align: center;
	}

	.head h1 {
		margin: 0;
		font-size: 1.4rem;
	}

	.head p {
		margin: 0;
		font-size: 0.9rem;
	}

	.mark {
		display: grid;
		place-items: center;
		margin-bottom: 4px;
	}

	.error {
		margin: 0;
		font-size: 0.9rem;
		color: var(--bad, #e5484d);
	}

	.foot {
		margin: 4px 0 0;
		text-align: center;
		font-size: 0.85rem;
	}
</style>
