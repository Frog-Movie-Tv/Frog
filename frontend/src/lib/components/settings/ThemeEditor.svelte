<script lang="ts">
	import Icon from '#lib/components/Icon.svelte';
	import { downloadJson } from '#lib/download';
	import { settings, type CustomTheme } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';

	/** The colour editor: tune the theme's tokens, save, share and reuse them. */
	let themeName = $state('My theme');
	let importing = $state(false);

	const s = $derived(settings.current);

	const tokens = [
		{ key: 'bg', label: 'Background' },
		{ key: 'bg-elev', label: 'Surface' },
		{ key: 'bg-elev-2', label: 'Raised surface' },
		{ key: 'accent', label: 'Accent' },
		{ key: 'text', label: 'Text' }
	] as const;

	/** Read the colour actually in force, so the pickers start where you are. */
	function currentToken(key: string): string {
		const override = s.customTheme?.tokens?.[key as keyof CustomTheme['tokens']];
		if (override) return override;
		if (typeof document === 'undefined') return '#000000';
		return toHex(getComputedStyle(document.documentElement).getPropertyValue(`--${key}`).trim());
	}

	/** Colour inputs only accept #rrggbb, so normalise whatever the token holds. */
	function toHex(value: string): string {
		if (value.startsWith('#')) {
			return value.length === 4
				? '#' + [...value.slice(1)].map((c) => c + c).join('')
				: value.slice(0, 7);
		}
		const m = value.match(/rgba?\(([^)]+)\)/);
		if (!m) return '#000000';
		const [r, g, b] = m[1].split(',').map((n) => Math.round(parseFloat(n)));
		return '#' + [r, g, b].map((n) => n.toString(16).padStart(2, '0')).join('');
	}

	function setToken(key: string, value: string) {
		const base: CustomTheme = s.customTheme ?? { name: themeName, base: s.theme, tokens: {} };
		settings.set('customTheme', {
			...base,
			base: s.theme,
			tokens: { ...base.tokens, [key]: value }
		});
	}

	function saveTheme() {
		if (!s.customTheme) {
			toasts.push('Change a colour first', 'error');
			return;
		}
		const theme: CustomTheme = { ...s.customTheme, name: themeName.trim() || 'My theme' };
		const rest = s.savedThemes.filter((t) => t.name !== theme.name);
		settings.patch({ savedThemes: [...rest, theme], customTheme: theme });
		toasts.push(`Saved “${theme.name}”`);
	}

	function applyTheme(theme: CustomTheme) {
		settings.patch({ theme: theme.base, customTheme: theme });
		themeName = theme.name;
	}

	/** Theme names identify saved themes, so a copy never reuses one. */
	function unusedName(base: string): string {
		const taken = new Set(s.savedThemes.map((t) => t.name));
		if (!taken.has(base)) return base;
		let n = 2;
		while (taken.has(`${base} ${n}`)) n++;
		return `${base} ${n}`;
	}

	function deleteTheme(theme: CustomTheme) {
		const before = s.savedThemes;
		settings.set(
			'savedThemes',
			before.filter((t) => t !== theme)
		);
		toasts.undoable(`Deleted “${theme.name}”`, () => settings.set('savedThemes', before));
	}

	function duplicateTheme(theme: CustomTheme) {
		const copy = { ...theme, name: unusedName(`${theme.name} copy`) };
		settings.patch({ savedThemes: [...s.savedThemes, copy], customTheme: copy });
		themeName = copy.name;
	}

	async function importTheme(file: File | null | undefined) {
		if (!file) return;
		importing = true;
		try {
			const parsed = JSON.parse(await file.text());
			if (!parsed || typeof parsed !== 'object' || !parsed.tokens) {
				throw new Error('That file is not a Frog theme');
			}
			const theme = parsed as CustomTheme;
			settings.patch({
				savedThemes: [...s.savedThemes.filter((t) => t.name !== theme.name), theme],
				customTheme: theme,
				theme: theme.base ?? s.theme
			});
			toasts.push(`Imported “${theme.name}”`);
		} catch (e) {
			toasts.error(e);
		} finally {
			importing = false;
		}
	}
</script>

<details class="editor">
	<summary><Icon name="chevron-right" size={16} />Colour editor</summary>

	<div class="colours">
		{#each tokens as token (token.key)}
			<label class="colour">
				<input
					type="color"
					value={currentToken(token.key)}
					oninput={(e) => setToken(token.key, e.currentTarget.value)}
				/>
				<span>{token.label}</span>
			</label>
		{/each}
	</div>

	<div class="actions">
		<input class="input name" bind:value={themeName} aria-label="Theme name" />
		<button class="btn" onclick={saveTheme}>Save</button>
		<button class="btn ghost" onclick={() => settings.set('customTheme', null)}>
			Reset colours
		</button>
		<button
			class="btn ghost"
			disabled={!s.customTheme}
			onclick={() =>
				downloadJson(
					`${(s.customTheme?.name ?? 'frog-theme').replace(/\s+/g, '-')}.json`,
					s.customTheme
				)}
		>
			Export
		</button>
		<label class="btn ghost file">
			Import
			<input
				type="file"
				accept="application/json"
				disabled={importing}
				onchange={(e) => importTheme(e.currentTarget.files?.[0])}
			/>
		</label>
	</div>

	{#if s.savedThemes.length}
		<ul class="saved">
			<!-- By position: a list saved before names were kept unique can still
			     hold two of one name, and must not break the page. -->
			{#each s.savedThemes as theme, i (i)}
				<li>
					<span class="dots" aria-hidden="true">
						{#each Object.entries(theme.tokens) as [token, c] (token)}<i style:background={c}
							></i>{/each}
					</span>
					<strong>{theme.name}</strong>
					<button class="link" onclick={() => applyTheme(theme)}>Use</button>
					<button class="link" onclick={() => duplicateTheme(theme)}>Duplicate</button>
					<button class="link danger" onclick={() => deleteTheme(theme)}>Delete</button>
				</li>
			{/each}
		</ul>
	{/if}
</details>

<style>
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

	.colours {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
		gap: 12px;
		margin: 16px 0;
	}

	.colour {
		display: flex;
		align-items: center;
		gap: 9px;
		font-size: 0.84rem;
		cursor: pointer;
	}

	input[type='color'] {
		width: 34px;
		height: 34px;
		padding: 0;
		border: 1px solid var(--line-strong);
		border-radius: 8px;
		background: none;
		cursor: pointer;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
		align-items: center;
	}

	.input.name {
		width: 160px;
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

	.saved {
		list-style: none;
		padding: 0;
		margin: 16px 0 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.saved li {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 0.86rem;
	}

	.dots {
		display: inline-flex;
		gap: 3px;
	}

	.dots i {
		width: 13px;
		height: 13px;
		border-radius: 4px;
		border: 1px solid var(--line);
	}

	.saved strong {
		flex: 1;
		font-weight: 600;
	}

	.link {
		background: none;
		border: none;
		color: var(--text-dim);
		cursor: pointer;
		font-size: 0.8rem;
		text-decoration: underline;
		padding: 2px;
	}

	.link:hover {
		color: var(--text);
	}

	.link.danger:hover {
		color: var(--bad);
	}
</style>
