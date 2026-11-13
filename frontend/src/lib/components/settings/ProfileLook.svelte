<script lang="ts">
	import {
		COLOUR_NAMES,
		PALETTE,
		PROFILE_COLOURS,
		PROFILE_SHADES,
		SHADE_NAMES,
		isColour,
		isShade,
		profileStyle,
		type ProfileShade
	} from '#lib/profile-colours';
	import { currentTone } from '#lib/settings.svelte';
	import type { Account, ProfileEdit } from '#lib/types';

	interface Props {
		account: Account | null;
		save: (edit: ProfileEdit, message?: string) => Promise<void>;
	}

	let { account, save }: Props = $props();

	const colour = $derived.by(() => {
		const c = account?.profile_color;
		return isColour(c) ? c : null;
	});
	const shade = $derived.by((): ProfileShade => {
		const s = account?.profile_shade;
		return isShade(s) ? s : 'medium';
	});
	const tone = $derived(currentTone());
	const preview = $derived(profileStyle(colour, shade, tone));
	const name = $derived(account?.display_name || account?.username || '');
</script>

<div class="look">
	<div class="head">
		<strong>Colour</strong>
		<span class="faint">Your profile wears it for everyone who opens it.</span>
	</div>

	<div class="swatches" role="radiogroup" aria-label="Profile colour">
		<button
			class="swatch none"
			class:on={!colour}
			role="radio"
			aria-checked={!colour}
			aria-label="The theme's own"
			title="The theme's own"
			onclick={() => save({ profile_color: '' })}
		></button>
		{#each PROFILE_COLOURS as c (c)}
			<button
				class="swatch"
				class:on={colour === c}
				role="radio"
				aria-checked={colour === c}
				aria-label={COLOUR_NAMES[c]}
				title={COLOUR_NAMES[c]}
				style:background={PALETTE[c][shade][tone]}
				onclick={() => save({ profile_color: c, profile_shade: shade })}
			></button>
		{/each}
	</div>

	{#if colour}
		<div class="segmented shades" role="radiogroup" aria-label="Shade">
			{#each PROFILE_SHADES as s (s)}
				<button
					class:on={shade === s}
					role="radio"
					aria-checked={shade === s}
					onclick={() => save({ profile_shade: s })}>{SHADE_NAMES[s]}</button
				>
			{/each}
		</div>
	{/if}

	<!-- Your profile in miniature, in the colour picked. -->
	<div class="preview" style={preview} aria-hidden="true">
		<span class="banner"></span>
		<span class="row">
			<span class="face">{name.slice(0, 1).toUpperCase()}</span>
			<span class="who">
				<b>{name}</b>
				<span class="taste"><strong>87%</strong> taste match</span>
			</span>
			<span class="button">Compare</span>
		</span>
		<span class="tiles">
			<span class="tile"><span>Films</span><b>42</b></span>
			<span class="tile"><span>Episodes</span><b>318</b></span>
			<span class="tile"><span>Streak</span><b>6 days</b></span>
		</span>
	</div>
</div>

<style>
	.look {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px 20px;
		border-top: 1px solid var(--line);
	}

	.head {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.head .faint {
		font-size: 0.84rem;
	}

	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.swatch {
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		cursor: pointer;
	}

	.swatch.none {
		background:
			linear-gradient(
				135deg,
				transparent 45%,
				var(--line-strong) 45%,
				var(--line-strong) 55%,
				transparent 55%
			),
			var(--bg-elev);
	}

	.swatch.on {
		outline: 2px solid var(--text);
		outline-offset: 2px;
	}

	.shades {
		align-self: flex-start;
	}

	.preview {
		display: flex;
		flex-direction: column;
		max-width: 420px;
		border-radius: var(--radius);
		overflow: hidden;
		border: 1px solid var(--line);
		background: var(--bg);
	}

	.banner {
		height: 64px;
		background:
			radial-gradient(circle at 20% 20%, var(--accent-soft), transparent 55%),
			linear-gradient(135deg, var(--bg-elev-2), var(--bg) 80%);
	}

	.row {
		display: flex;
		align-items: flex-end;
		gap: 10px;
		margin-top: -22px;
		padding: 0 12px;
		min-width: 0;
	}

	.face {
		flex: none;
		display: grid;
		place-items: center;
		width: 44px;
		height: 44px;
		border-radius: 50%;
		border: 3px solid var(--bg);
		box-shadow: 0 0 0 2px var(--accent);
		background: var(--accent-soft);
		color: var(--accent);
		font-weight: 700;
	}

	.who {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
		font-size: 0.82rem;
	}

	.who b {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.taste {
		color: var(--text-dim);
		font-size: 0.74rem;
	}

	.taste strong {
		color: var(--accent);
	}

	.button {
		flex: none;
		padding: 4px 10px;
		border-radius: 999px;
		background: var(--accent);
		color: var(--accent-ink);
		font-size: 0.74rem;
		font-weight: 650;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 6px;
		padding: 10px 12px 12px;
	}

	.tile {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 6px 8px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--accent) 8%, var(--bg-elev));
		font-size: 0.7rem;
		min-width: 0;
	}

	.tile span {
		color: var(--accent);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		font-size: 0.62rem;
		font-weight: 650;
	}
</style>
