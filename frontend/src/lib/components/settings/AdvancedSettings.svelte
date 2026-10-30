<script lang="ts">
	import Icon from '#lib/components/Icon.svelte';
	import { downloadJson } from '#lib/download';
	import { DEFAULTS, settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';

	/** Advanced: custom CSS, and every setting as one file to keep or restore. */
	const s = $derived(settings.current);
	let importing = $state(false);

	async function importSettings(file: File | null | undefined) {
		if (!file) return;
		importing = true;
		try {
			const parsed = JSON.parse(await file.text());
			settings.replace({ ...DEFAULTS, ...parsed });
			toasts.push('Settings restored');
		} catch (e) {
			toasts.error(e);
		} finally {
			importing = false;
		}
	}
</script>

<section class="block">
	<div class="section-head"><h2>Advanced</h2></div>

	<details class="editor">
		<summary><Icon name="chevron-right" size={16} />Custom CSS</summary>
		<p class="faint tiny">
			Applied to every page, for example <code>:root &#123; --accent: hotpink; &#125;</code>
		</p>
		<textarea
			class="input code"
			rows="7"
			spellcheck="false"
			placeholder=":root &#123; --radius: 4px; &#125;"
			value={s.customCss}
			onchange={(e) => settings.set('customCss', e.currentTarget.value)}></textarea>
	</details>

	<div class="actions">
		<button class="btn" onclick={() => downloadJson('frog-settings.json', s)}>
			Export all settings
		</button>
		<label class="btn ghost file">
			Import settings
			<input
				type="file"
				accept="application/json"
				disabled={importing}
				onchange={(e) => importSettings(e.currentTarget.files?.[0])}
			/>
		</label>
		<button class="btn ghost" onclick={() => settings.reset()}>Reset everything</button>
	</div>
</section>

<style>
	.block {
		margin-bottom: 44px;
	}

	.editor {
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		padding: 14px 18px;
		margin-bottom: 14px;
	}

	/* Frog's own chevron, not the browser's triangle, turning as it opens. */
	summary {
		display: flex;
		align-items: center;
		gap: 8px;
		cursor: pointer;
		font-weight: 600;
		font-size: 0.92rem;
		list-style: none;
	}

	summary::-webkit-details-marker {
		display: none;
	}

	summary :global(svg) {
		transition: transform var(--fast) var(--ease);
	}

	details[open] > summary :global(svg) {
		transform: rotate(90deg);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
		align-items: center;
	}

	.btn.file {
		position: relative;
		overflow: hidden;
	}

	.btn.file input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	.code {
		font-family: var(--font-mono);
		font-size: 0.8rem;
		margin-top: 10px;
		resize: vertical;
	}

	.tiny {
		font-size: 0.76rem;
		max-width: 70ch;
	}

	code {
		font-family: var(--font-mono);
		background: var(--bg-elev-2);
		padding: 1px 5px;
		border-radius: 4px;
	}
</style>
