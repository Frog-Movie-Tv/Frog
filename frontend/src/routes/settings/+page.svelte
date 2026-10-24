<script lang="ts">
	import { goto } from '$app/navigation';
	import ScoreInput from '#lib/components/ScoreInput.svelte';
	import { SCORE_FORMATS, type ScoreFormat } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings, type ThemeName } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';

	const themes: { id: ThemeName; label: string; hint: string }[] = [
		{ id: 'dark', label: 'Dark', hint: 'Deep charcoal' },
		{ id: 'amoled', label: 'AMOLED', hint: 'True black' },
		{ id: 'light', label: 'Light', hint: 'Paper' }
	];

	let preview = $state<number | null>(78);

	async function signOut() {
		await session.logout();
		toasts.push('Signed out');
		await goto('/login', { replaceState: true });
	}
</script>

<div class="page">
	<h1 class="title">Settings</h1>

	<section class="block">
		<div class="section-head"><h2>Appearance</h2></div>

		<div class="themes">
			{#each themes as t (t.id)}
				<button
					class="theme"
					class:on={settings.current.theme === t.id}
					data-theme-preview={t.id}
					onclick={() => settings.set('theme', t.id)}
				>
					<span class="swatches" aria-hidden="true">
						<i class="s1"></i><i class="s2"></i><i class="s3"></i>
					</span>
					<strong>{t.label}</strong>
					<span class="faint">{t.hint}</span>
				</button>
			{/each}
		</div>

		<label class="row card">
			<div class="label">
				<strong>Poster density</strong>
				<span class="faint">Smaller posters mean more of them per row.</span>
			</div>
			<input
				type="range"
				min="90"
				max="200"
				step="2"
				value={settings.current.posterSize}
				oninput={(e) => settings.set('posterSize', Number(e.currentTarget.value))}
			/>
		</label>

		<label class="row card">
			<div class="label">
				<strong>Follow the poster</strong>
				<span class="faint">Tint each title page with a colour taken from its poster.</span>
			</div>
			<input
				type="checkbox"
				checked={settings.current.followPoster}
				onchange={(e) => settings.set('followPoster', e.currentTarget.checked)}
			/>
		</label>
	</section>

	<section class="block">
		<div class="section-head"><h2>Score format</h2></div>

		<div class="formats">
			{#each SCORE_FORMATS as f (f.id)}
				<button
					class="format"
					class:on={settings.current.scoreFormat === f.id}
					onclick={() => settings.set('scoreFormat', f.id as ScoreFormat)}
				>
					<strong>{f.label}</strong>
					<span class="faint">{f.example}</span>
				</button>
			{/each}
		</div>

		<div class="card preview">
			<span class="eyebrow">Live preview</span>
			<ScoreInput value={preview} onchange={(v) => (preview = v)} />
			<p class="faint tiny">
				Ratings are stored on a 0–100 scale, so switching format never changes or rounds what you
				already rated.
			</p>
		</div>
	</section>

	<section class="block">
		<div class="section-head"><h2>This instance</h2></div>

		<dl class="facts card">
			<div><dt>Signed in as</dt><dd>{session.status?.username ?? '—'}</dd></div>
			<div><dt>Version</dt><dd>{session.status?.version ?? '—'}</dd></div>
			<div><dt>Provider region</dt><dd>{session.status?.region ?? '—'}</dd></div>
			<div>
				<dt>TMDB key</dt>
				<dd class={session.status?.tmdb_configured ? 'ok' : 'bad'}>
					{session.status?.tmdb_configured ? 'configured' : 'missing'}
				</dd>
			</div>
		</dl>

		<div class="actions">
			<button class="btn" onclick={() => settings.reset()}>Reset preferences</button>
			<button class="btn ghost" onclick={signOut}>Sign out</button>
		</div>
	</section>

	<p class="faint tiny">
		The theme editor, custom shelves, layouts and shareable theme JSON arrive in phase 3. These
		preferences live in this browser for now.
	</p>
</div>

<style>
	.title {
		margin-bottom: 22px;
	}

	.block {
		margin-bottom: 42px;
	}

	.themes {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: 12px;
		margin-bottom: 12px;
	}

	.theme {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 14px;
		border-radius: var(--radius);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		cursor: pointer;
		text-align: left;
		transition: border-color var(--fast) var(--ease);
	}

	.theme.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}

	.swatches {
		display: flex;
		gap: 4px;
		margin-bottom: 6px;
	}

	.swatches i {
		width: 20px;
		height: 20px;
		border-radius: 6px;
		border: 1px solid var(--line);
	}

	/* Each tile shows the palette it would switch to, not the current one. */
	[data-theme-preview='dark'] .s1 {
		background: #08080c;
	}
	[data-theme-preview='dark'] .s2 {
		background: #191922;
	}
	[data-theme-preview='amoled'] .s1 {
		background: #000;
	}
	[data-theme-preview='amoled'] .s2 {
		background: #121217;
	}
	[data-theme-preview='light'] .s1 {
		background: #f6f5f8;
	}
	[data-theme-preview='light'] .s2 {
		background: #fff;
	}
	.s3 {
		background: var(--accent);
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		padding: 16px 18px;
		margin-bottom: 10px;
		cursor: pointer;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.label .faint {
		font-size: 0.8rem;
	}

	input[type='range'] {
		accent-color: var(--accent);
		min-width: 150px;
	}

	input[type='checkbox'] {
		width: 20px;
		height: 20px;
		accent-color: var(--accent);
	}

	.formats {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: 10px;
		margin-bottom: 14px;
	}

	.format {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 12px 14px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		cursor: pointer;
		text-align: left;
		font-size: 0.86rem;
	}

	.format.on {
		border-color: var(--accent);
		background: var(--accent-soft);
	}

	.preview {
		padding: 18px;
		display: flex;
		flex-direction: column;
		gap: 12px;
		align-items: flex-start;
	}

	.facts {
		margin: 0 0 14px;
		padding: 6px 18px;
	}

	.facts div {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		padding: 12px 0;
		border-bottom: 1px solid var(--line);
		font-size: 0.88rem;
	}

	.facts div:last-child {
		border-bottom: none;
	}

	dt {
		color: var(--text-dim);
	}

	dd {
		margin: 0;
		font-weight: 550;
	}

	dd.ok {
		color: var(--good);
	}
	dd.bad {
		color: var(--bad);
	}

	.actions {
		display: flex;
		gap: 10px;
		flex-wrap: wrap;
	}

	.tiny {
		font-size: 0.76rem;
		max-width: 70ch;
	}
</style>
