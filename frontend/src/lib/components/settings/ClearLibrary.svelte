<script lang="ts">
	import { api } from '#lib/api';
	import { toasts } from '#lib/toast.svelte';

	// Emptying the library. The phrase has to be typed out: a button alone is
	// too easy to press by accident for something with no undo.
	const CLEAR_PHRASE = 'delete my library';
	let open = $state(false);
	let phrase = $state('');
	let password = $state('');
	let busy = $state(false);
	const ready = $derived(phrase.trim().toLowerCase() === CLEAR_PHRASE && password.length > 0);

	function close() {
		open = false;
		phrase = '';
		password = '';
	}

	async function clearLibrary() {
		busy = true;
		try {
			const gone = await api.clearLibrary(password, phrase);
			close();
			toasts.push(
				gone.titles === 0
					? 'There was nothing left to remove.'
					: `Removed ${gone.titles} ${gone.titles === 1 ? 'title' : 'titles'}.`
			);
		} catch (e) {
			toasts.push(e instanceof Error ? e.message : 'Could not empty your library');
		} finally {
			busy = false;
		}
	}
</script>

<section class="block">
	<div class="section-head"><h2>Start again</h2></div>

	<div class="danger-zone card">
		<div class="label">
			<strong>Empty your library</strong>
			<span class="faint">
				Removes every title you track, everything you have watched, your episode ratings and notes,
				and your shelves. Your account, your settings and your devices stay. This cannot be undone.
			</span>
		</div>

		{#if !open}
			<div class="actions">
				<a class="btn ghost" href="/api/export/library.json" download> Download a backup first </a>
				<button class="btn danger-btn" onclick={() => (open = true)}> Empty my library </button>
			</div>
		{:else}
			<div class="confirm">
				<label class="field">
					<span>Type <code>{CLEAR_PHRASE}</code> to confirm</span>
					<input
						class="input"
						bind:value={phrase}
						autocomplete="off"
						autocapitalize="none"
						spellcheck="false"
						placeholder={CLEAR_PHRASE}
					/>
				</label>

				<label class="field">
					<span>Your password</span>
					<input
						class="input"
						type="password"
						bind:value={password}
						autocomplete="current-password"
					/>
				</label>

				<div class="actions">
					<button class="btn ghost" onclick={close} disabled={busy}> Keep my library </button>
					<button class="btn danger-btn" onclick={clearLibrary} disabled={!ready || busy}>
						{busy ? 'Emptying…' : 'Empty it for good'}
					</button>
				</div>
			</div>
		{/if}
	</div>
</section>

<style>
	.block {
		margin-bottom: 44px;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.78rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
		align-items: center;
	}

	/* Visibly not an ordinary row: this one does not come back. */
	.danger-zone {
		display: grid;
		gap: 14px;
		padding: 18px;
		border: 1px solid color-mix(in oklab, var(--bad) 35%, transparent);
	}

	.danger-zone .label {
		display: grid;
		gap: 5px;
	}

	.danger-zone .faint {
		max-width: 60ch;
		line-height: 1.5;
	}

	.danger-btn {
		background: var(--bad);
		border-color: transparent;
		color: var(--bad-ink);
	}

	.danger-btn:hover:not(:disabled) {
		filter: brightness(1.08);
	}

	.danger-btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
		filter: none;
	}

	.confirm {
		display: grid;
		gap: 13px;
		padding-top: 14px;
		border-top: 1px solid var(--line, rgba(128, 128, 128, 0.25));
	}

	.confirm code {
		font-size: 0.92em;
	}

	code {
		font-family: var(--font-mono);
		background: var(--bg-elev-2);
		padding: 1px 5px;
		border-radius: 4px;
	}
</style>
