<script lang="ts">
	import { api } from '#lib/api';
	import { toasts } from '#lib/toast.svelte';

	// Emptying the library. The phrase has to be typed out: a button alone is
	// too easy to press by accident for something with no undo. All of it stays
	// folded away until asked for, so the page does not lead with it.
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
	<div class="rows list-pane" class:open>
		<div class="row">
			<span class="label">
				<strong>Empty your library</strong>
				<span class="faint">Start again with nothing tracked</span>
			</span>
			{#if !open}
				<button class="btn ghost small danger-text" onclick={() => (open = true)}>Empty…</button>
			{/if}
		</div>

		{#if open}
			<div class="confirm">
				<p class="warning">
					Every title, watch, note and shelf goes. Your account and settings stay. This cannot be
					undone. <a href="/api/export/library.json" download>Download a backup first</a>.
				</p>

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
					<button class="btn ghost" onclick={close} disabled={busy}>Keep my library</button>
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
		margin-bottom: 36px;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		padding: 15px 18px;
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

	/* Only once opened does it look like what it is. */
	.list-pane.open {
		border-color: color-mix(in oklab, var(--bad) 35%, transparent);
	}

	.btn.small {
		flex: none;
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.danger-text {
		color: var(--bad);
	}

	.confirm {
		display: grid;
		gap: 13px;
		padding: 0 18px 18px;
	}

	.warning {
		margin: 0;
		max-width: 60ch;
		font-size: 0.84rem;
		line-height: 1.5;
		color: var(--text-dim);
	}

	.warning a {
		color: var(--accent);
		text-decoration: underline;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
		align-items: center;
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

	code {
		font-family: var(--font-mono);
		font-size: 0.92em;
		background: var(--bg-elev-2);
		padding: 1px 5px;
		border-radius: 4px;
	}
</style>
