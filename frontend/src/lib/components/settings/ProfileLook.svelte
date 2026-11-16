<script lang="ts">
	import {
		COLOUR_NAMES,
		PALETTE,
		PROFILE_COLOURS,
		PROFILE_SHADES,
		SHADE_NAMES,
		isColour,
		isShade,
		type ProfileShade
	} from '#lib/profile-colours';
	import { currentTone } from '#lib/settings.svelte';
	import Icon from '../Icon.svelte';
	import BannerFocus, { type BannerShape } from './BannerFocus.svelte';
	import type { Account, ProfileEdit } from '#lib/types';

	interface Props {
		account: Account | null;
		save: (edit: ProfileEdit, message?: string) => Promise<void>;
		/** The banner's picture, when there is one. */
		banner?: string | null;
	}

	let { account, save, banner = null }: Props = $props();

	const colour = $derived.by(() => {
		const c = account?.profile_color;
		return isColour(c) ? c : null;
	});
	const shade = $derived.by((): ProfileShade => {
		const s = account?.profile_shade;
		return isShade(s) ? s : 'medium';
	});
	const tone = $derived(currentTone());
	const layout = $derived(account?.banner_layout ?? 'classic');
	const height = $derived(account?.banner_height ?? 'short');

	const LAYOUTS = [
		{ id: 'classic', label: 'Across the top', about: 'A wide banner over your profile.' },
		{
			id: 'vertical',
			label: 'A column beside it',
			about: 'A portrait beside your profile, across the top on a phone.'
		}
	] as const;

	// Each layout keeps its own part of the picture: the column a strip,
	// across the top a band. Across the top follows the column's until it is
	// chosen, as both shared one before.
	let choosing = $state(false);
	const focus = $derived(
		layout === 'vertical'
			? account?.banner_position
			: (account?.banner_top_position ?? account?.banner_position)
	);
	const shape = $derived<BannerShape>(
		layout === 'vertical'
			? 'column'
			: height === 'tall'
				? 'tall'
				: height === 'full'
					? 'full'
					: 'short'
	);

	function saveFocus(position: string) {
		void save(
			layout === 'vertical' ? { banner_position: position } : { banner_top_position: position },
			'Banner position saved'
		);
	}
</script>

<div class="setting">
	<div class="setting-top">
		<span class="setting-tile" aria-hidden="true"><Icon name="palette" size={18} /></span>
		<span class="setting-label">
			<strong>Colour</strong>
			<span class="faint">Your profile wears it for everyone who opens it.</span>
		</span>
	</div>
	<div class="setting-body colours">
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
				>
					{#if colour === c}<Icon name="check" size={14} />{/if}
				</button>
			{/each}
		</div>
		{#if colour}
			<div class="segmented" role="radiogroup" aria-label="Shade">
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
	</div>
</div>

<div class="setting">
	<div class="setting-top">
		<span class="setting-tile" aria-hidden="true"><Icon name="layers" size={18} /></span>
		<span class="setting-label">
			<strong>Banner layout</strong>
			<span class="faint">How your banner sits on your profile. A tap on it opens it whole.</span>
		</span>
	</div>
	<div class="setting-body banner-choices">
		<div class="layouts" role="radiogroup" aria-label="Banner layout">
			{#each LAYOUTS as l (l.id)}
				{@const on = layout === l.id}
				<button
					class="layout"
					class:on
					role="radio"
					aria-checked={on}
					onclick={() => save({ banner_layout: l.id === 'classic' ? '' : l.id })}
				>
					<span
						class="sketch {l.id}"
						class:tall={l.id === 'classic' && height !== 'short'}
						aria-hidden="true"
					>
						<i
							class="pic"
							class:empty={!banner}
							style:background-image={banner ? `url("${banner}")` : undefined}
							style:background-position={(l.id === 'vertical'
								? account?.banner_position
								: (account?.banner_top_position ?? account?.banner_position)) ?? 'center'}
						></i>
						<i class="dot"></i><i class="line"></i><i class="line short"></i><i class="tiles"></i>
					</span>
					<span class="caption">
						<span class="name">
							{l.label}
							{#if on}<span class="chosen"><Icon name="check" size={12} /> In use</span>{/if}
						</span>
						<span class="about">{l.about}</span>
					</span>
				</button>
			{/each}
		</div>

		<div class="extras">
			{#if layout !== 'vertical'}
				<div class="height">
					<span class="small">Height</span>
					<div class="segmented" role="radiogroup" aria-label="Banner height">
						{#each [['short', 'Short'], ['tall', 'Tall'], ['full', 'Full']] as [id, label] (id)}
							<button
								class:on={height === id}
								role="radio"
								aria-checked={height === id}
								onclick={() => save({ banner_height: id === 'short' ? '' : id })}>{label}</button
							>
						{/each}
					</div>
				</div>
			{/if}
			{#if banner}
				<button class="btn" onclick={() => (choosing = true)}>
					<Icon name="move" size={15} /> Choose what shows
				</button>
			{/if}
		</div>
	</div>
</div>

{#if choosing && banner}
	<BannerFocus
		src={banner}
		{shape}
		position={focus}
		onsave={saveFocus}
		onclose={() => (choosing = false)}
	/>
{/if}

<style>
	.colours,
	.banner-choices {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 12px;
	}

	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.swatch {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		color: #fff;
		cursor: pointer;
		transition: transform var(--fast) var(--ease);
	}

	.swatch:hover {
		transform: scale(1.08);
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

	/* Each layout as a small picture of the page it makes, drawn with your
	   own banner, so the one in use is plain to see. */
	.layouts {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 240px));
		gap: 12px;
		width: 100%;
	}

	.layout {
		position: relative;
		display: grid;
		gap: 10px;
		padding: 8px 8px 12px;
		border-radius: var(--radius);
		border: 1.5px solid var(--line);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition:
			border-color var(--fast) var(--ease),
			box-shadow var(--fast) var(--ease),
			transform var(--fast) var(--ease);
	}

	.layout:hover {
		border-color: var(--line-strong);
		transform: translateY(-1px);
	}

	.layout.on {
		border-color: var(--accent);
		color: var(--text);
		box-shadow: 0 0 0 3px var(--accent-soft);
	}

	.caption {
		display: grid;
		gap: 2px;
		padding: 0 4px;
	}

	.name {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px 8px;
		color: var(--text);
		font-size: 0.9rem;
		font-weight: 600;
	}

	.chosen {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 1px 8px 1px 6px;
		border-radius: 999px;
		background: var(--accent);
		color: var(--accent-ink);
		font-size: 0.72rem;
		font-weight: 700;
	}

	.about {
		font-size: 0.8rem;
		line-height: 1.35;
	}

	.sketch {
		position: relative;
		display: block;
		aspect-ratio: 16 / 10;
		border-radius: calc(var(--radius) - 4px);
		background: var(--bg);
		overflow: hidden;
	}

	.layout:not(.on) .sketch {
		opacity: 0.75;
	}

	.sketch i {
		position: absolute;
		display: block;
		border-radius: 3px;
		background: var(--line-strong);
	}

	.sketch .pic {
		background: var(--bg-elev-2) center / cover no-repeat;
	}

	.sketch .pic.empty {
		background-image: linear-gradient(135deg, var(--accent-soft), var(--bg-elev-2));
	}

	.sketch .dot {
		width: 14%;
		aspect-ratio: 1;
		border-radius: 50%;
		background: var(--text-faint);
		border: 2px solid var(--bg);
	}

	.sketch.classic .pic {
		inset: 0 0 auto 0;
		height: 38%;
		border-radius: 0;
		transition: height var(--fast) var(--ease);
	}

	.sketch.classic.tall .pic {
		height: 50%;
	}

	.sketch.classic .dot {
		left: 7%;
		top: 28%;
	}

	.sketch.classic.tall .dot {
		top: 40%;
	}

	.sketch.classic .line {
		left: 25%;
		top: 44%;
		width: 36%;
		height: 6%;
	}

	.sketch.classic.tall .line {
		top: 55%;
	}

	.sketch.classic .line.short {
		top: 54%;
		width: 22%;
	}

	.sketch.classic.tall .line.short {
		top: 64%;
	}

	.sketch.classic .tiles {
		left: 7%;
		right: 7%;
		bottom: 8%;
		height: 18%;
	}

	.sketch.vertical .pic {
		left: 5%;
		top: 7%;
		bottom: 7%;
		width: 30%;
		border-radius: 6px;
	}

	.sketch.vertical .dot {
		left: 9%;
		bottom: 12%;
		width: 10%;
	}

	.sketch.vertical .line {
		left: 41%;
		top: 12%;
		width: 34%;
		height: 6%;
	}

	.sketch.vertical .line.short {
		top: 22%;
		width: 22%;
	}

	.sketch.vertical .tiles {
		left: 41%;
		right: 6%;
		top: 36%;
		bottom: 9%;
	}

	.extras {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px 16px;
	}

	.height {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.small {
		font-size: 0.82rem;
	}
</style>
