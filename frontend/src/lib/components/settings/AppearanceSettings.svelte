<script lang="ts">
	import Select from '#lib/components/Select.svelte';
	import ThemeEditor from '#lib/components/settings/ThemeEditor.svelte';
	import {
		FONTS,
		MASCOTS,
		THEMES,
		settings,
		type AnimationLevel,
		type CardStyle,
		type FontName,
		type HeroStyle,
		type MascotName,
		type ThemeName
	} from '#lib/settings.svelte';

	/** Appearance: theme, mascot, type, cards and title pages. */
	const s = $derived(settings.current);

	const cardToggles = [
		['showBadge', 'Status badge'],
		['showRating', 'My rating'],
		['showProgress', 'Progress bar'],
		['showYear', 'Year']
	] as const;
</script>

<section class="block">
	<div class="section-head"><h2>Theme</h2></div>

	<div class="tiles">
		{#each THEMES as t (t.id)}
			<button
				class="tile"
				class:on={s.theme === t.id}
				aria-label={t.label}
				aria-pressed={s.theme === t.id}
				onclick={() => settings.patch({ theme: t.id as ThemeName, customTheme: null })}
			>
				<span class="swatches" aria-hidden="true">
					{#each t.swatches as c (c)}<i style:background={c}></i>{/each}
				</span>
				<strong>{t.label}</strong>
				<span class="faint">{t.hint}</span>
			</button>
		{/each}
	</div>

	<ThemeEditor />
</section>

<section class="block">
	<div class="section-head"><h2>Mascot</h2></div>

	<div class="tiles mascots">
		{#each MASCOTS as m (m.id)}
			<button
				class="tile"
				class:on={s.mascot === m.id}
				aria-label={m.label}
				aria-pressed={s.mascot === m.id}
				onclick={() => settings.set('mascot', m.id as MascotName)}
			>
				<img src="/mascots/{m.id}.svg" alt="" width="44" height="44" />
				<strong>{m.label}</strong>
			</button>
		{/each}
	</div>
	<p class="faint tiny">Reinstall the app to change its home-screen icon.</p>
</section>

<section class="block">
	<div class="section-head"><h2>Type and shape</h2></div>

	<div class="rows">
		<div class="row card">
			<span class="label"><strong>Font</strong></span>
			<Select
				label="Font"
				value={s.font}
				options={FONTS.map((f) => ({ value: f.id, label: f.label }))}
				onchange={(v) => settings.set('font', v as FontName)}
			/>
		</div>

		<label class="row card">
			<span class="label">
				<strong>Text size</strong><span class="faint">{Math.round(s.textScale * 100)}%</span>
			</span>
			<input
				type="range"
				min="0.85"
				max="1.3"
				step="0.05"
				value={s.textScale}
				oninput={(e) => settings.set('textScale', Number(e.currentTarget.value))}
			/>
		</label>

		<div class="row card">
			<span class="label">
				<strong>Animation</strong>
			</span>
			<Select
				label="Animation"
				value={s.animations}
				options={[
					{ value: 'full', label: 'Full' },
					{ value: 'reduced', label: 'Reduced' },
					{ value: 'off', label: 'Off' }
				]}
				onchange={(v) => settings.set('animations', v as AnimationLevel)}
			/>
		</div>
	</div>
</section>

<section class="block">
	<div class="section-head"><h2>Cards and grid</h2></div>

	<div class="rows">
		<div class="row card">
			<span class="label">
				<strong>Card style</strong>
			</span>
			<Select
				label="Card style"
				value={s.cardStyle}
				options={[
					{ value: 'poster', label: 'Poster' },
					{ value: 'landscape', label: 'Landscape' },
					{ value: 'list', label: 'List' },
					{ value: 'compact', label: 'Compact table' }
				]}
				onchange={(v) => settings.set('cardStyle', v as CardStyle)}
			/>
		</div>

		<label class="row card">
			<span class="label">
				<strong>Poster size</strong><span class="faint">{s.posterSize}px columns</span>
			</span>
			<input
				type="range"
				min="90"
				max="230"
				step="2"
				value={s.posterSize}
				oninput={(e) => settings.set('posterSize', Number(e.currentTarget.value))}
			/>
		</label>

		{#each cardToggles as [key, label] (key)}
			<label class="row card">
				<span class="label"><strong>{label}</strong></span>
				<input
					type="checkbox"
					class="toggle-switch"
					checked={s[key]}
					onchange={(e) => settings.set(key, e.currentTarget.checked)}
				/>
			</label>
		{/each}
	</div>
</section>

<section class="block">
	<div class="section-head"><h2>Title pages</h2></div>

	<div class="rows">
		<div class="row card">
			<span class="label">
				<strong>Hero</strong>
			</span>
			<Select
				label="Hero"
				value={s.heroStyle}
				options={[
					{ value: 'backdrop', label: 'Backdrop' },
					{ value: 'poster', label: 'Poster' },
					{ value: 'off', label: 'Off' }
				]}
				onchange={(v) => settings.set('heroStyle', v as HeroStyle)}
			/>
		</div>

		<label class="row card">
			<span class="label">
				<strong>Follow the poster</strong>
				<span class="faint">Colour each title page from its artwork</span>
			</span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.followPoster}
				onchange={(e) => settings.set('followPoster', e.currentTarget.checked)}
			/>
		</label>
	</div>
</section>

<style>
	.block {
		margin-bottom: 44px;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(158px, 1fr));
		gap: 12px;
		margin-bottom: 14px;
	}

	.tile {
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

	.tile.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}

	.tile .faint {
		font-size: 0.76rem;
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

	.mascots img {
		margin-bottom: 4px;
	}

	.rows {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		padding: 15px 18px;
		cursor: pointer;
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

	input[type='range'] {
		accent-color: var(--accent);
		min-width: 140px;
		flex: none;
	}

	.tiny {
		font-size: 0.76rem;
		max-width: 70ch;
	}

	@media (max-width: 720px) {
		.row {
			align-items: flex-start;
			flex-direction: column;
			gap: 10px;
		}
	}
</style>
