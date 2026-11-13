<script lang="ts" module>
	export type ProfileTab = 'overview' | 'library' | 'ratings' | 'compare';
</script>

<script lang="ts">
	import { img } from '#lib/api';
	import Avatar from './Avatar.svelte';
	import Badges from './Badges.svelte';
	import Icon from './Icon.svelte';
	import { prettyDate } from '#lib/format';
	import { backCloses } from '#lib/layers.svelte';
	import { portal } from '#lib/portal';
	import type { Profile } from '#lib/types';

	interface Props {
		profile: Profile;
		tab: ProfileTab;
	}

	let { profile, tab }: Props = $props();

	const banner = $derived(
		profile.banner ?? (profile.banner_backdrop ? img(profile.banner_backdrop, 'w1280') : null)
	);
	/** The same picture at its largest, for opening it whole. */
	const whole = $derived(
		profile.banner ?? (profile.banner_backdrop ? img(profile.banner_backdrop, 'original') : null)
	);
	const vertical = $derived(profile.banner_layout === 'vertical');
	const height = $derived(profile.banner_height ?? 'short');
	let opened = $state(false);
	backCloses(
		() => opened,
		() => (opened = false)
	);
	const shownName = $derived(profile.display_name || profile.username);
	const base = $derived(`/people/${encodeURIComponent(profile.username)}`);

	const tabs = $derived([
		{ id: 'overview', label: 'Overview', href: base },
		{ id: 'library', label: 'Library', href: `${base}/library` },
		{ id: 'ratings', label: 'Ratings', href: `${base}/ratings` },
		...(profile.me ? [] : [{ id: 'compare', label: 'Compare', href: `${base}/compare` }])
	] as { id: ProfileTab; label: string; href: string }[]);
</script>

<svelte:window
	onkeydown={(e) => {
		if (opened && e.key === 'Escape') opened = false;
	}}
/>

<header
	class="hero"
	class:vertical
	class:tall={height === 'tall' && !vertical}
	class:full={height === 'full' && !vertical && !!banner}
>
	<div class="side">
		{#if banner}
			<button class="banner" aria-label="Open the banner" onclick={() => (opened = true)}>
				<img src={banner} alt="" style:object-position={profile.banner_position ?? undefined} />
				<span class="shade" aria-hidden="true"></span>
			</button>
		{:else}
			<div class="banner plain"><span class="shade" aria-hidden="true"></span></div>
		{/if}

		<div class="identity">
			<span class="picture">
				<Avatar src={profile.avatar} name={profile.username} size={vertical ? 96 : 112} />
			</span>
			<div class="names">
				<div class="name-line">
					<h1 title={shownName}>{shownName}</h1>
					<Badges badges={profile.badges ?? []} size={28} />
				</div>
				<p class="faint handle">
					<span class="at" title="@{profile.username}">@{profile.username}</span>
					<span>· joined {prettyDate(profile.joined)}</span>
				</p>
				{#if profile.taste}
					<p class="taste" title={`From the ${profile.taste.shared} titles you have both rated`}>
						<strong>{profile.taste.percent}%</strong> taste match
					</p>
				{/if}
			</div>
			{#if profile.me}
				<a class="btn ghost edit" href="/settings#account">
					<Icon name="pencil" size={15} /> Edit profile
				</a>
			{/if}
		</div>

		{#if profile.bio}
			<p class="bio">{profile.bio}</p>
		{/if}
		{#if profile.me && !profile.public}
			<p class="private faint">
				Only you can see this. <a href="/settings#account">Show it to others</a>
			</p>
		{/if}
	</div>

	<nav class="tabs segmented" aria-label="Profile">
		{#each tabs as t (t.id)}
			<a href={t.href} class:on={tab === t.id} aria-current={tab === t.id ? 'page' : undefined}>
				{t.label}
			</a>
		{/each}
	</nav>
</header>

{#if opened && whole}
	<!-- The whole picture, uncropped. A tap anywhere, Escape or Back closes it. -->
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div
		use:portal
		class="whole"
		role="dialog"
		aria-modal="true"
		aria-label="Banner"
		tabindex="-1"
		onclick={() => (opened = false)}
	>
		<img src={whole} alt="" />
	</div>
{/if}

<style>
	.hero {
		position: relative;
		/* Out to the page's edges and up to the bar, so the banner is the top
		   of the page rather than a picture on it. */
		margin: -24px calc(var(--page-pad) * -1) 28px;
		animation: rise 420ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}

	.banner {
		position: relative;
		height: clamp(180px, 28vw, 320px);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	/* The banner is a button: a tap opens the whole picture. */
	button.banner {
		display: block;
		width: 100%;
		padding: 0;
		border: 0;
		cursor: zoom-in;
		font: inherit;
		color: inherit;
	}

	button.banner:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: -4px;
	}

	.tall .banner {
		height: clamp(240px, 44vw, 560px);
	}

	/* Full: the whole picture, as tall as it is. */
	.full .banner {
		height: auto;
	}

	/* As much of the picture as fits under a ceiling: a tall one is cut
	   there, where its position says. */
	.full .banner img {
		height: auto;
		max-height: min(80vh, 860px);
		display: block;
	}

	.banner.plain {
		background:
			radial-gradient(circle at 20% 20%, var(--accent-soft), transparent 55%),
			linear-gradient(135deg, var(--bg-elev-2), var(--bg) 80%);
	}

	/* With no picture, a profile in its own colour fills the banner with it. */
	:global(.coloured) .banner.plain {
		background:
			radial-gradient(
				circle at 18% 22%,
				color-mix(in srgb, var(--accent) 48%, transparent),
				transparent 62%
			),
			linear-gradient(
				135deg,
				color-mix(in srgb, var(--accent) 24%, var(--bg-elev-2)),
				var(--bg) 88%
			);
	}

	.banner img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		animation: settle 900ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	@keyframes settle {
		from {
			transform: scale(1.06);
		}
	}

	/* The page grows out of the picture, so the name below reads on the page. */
	.shade {
		position: absolute;
		inset: 0;
		background: linear-gradient(to top, var(--bg), transparent 55%);
	}

	.identity {
		position: relative;
		z-index: 1;
		display: flex;
		align-items: flex-end;
		gap: 20px;
		max-width: 1440px;
		margin: -64px auto 0;
		padding: 0 var(--page-pad);
	}

	.picture :global(.avatar) {
		border: 4px solid var(--bg);
		box-shadow: var(--shadow-3);
	}

	/* A profile in its own colour wears it round the picture too. */
	:global(.coloured) .picture :global(.avatar) {
		box-shadow:
			0 0 0 3px var(--accent),
			var(--shadow-3);
	}

	.names {
		flex: 1;
		min-width: 0;
		padding-bottom: 6px;
	}

	.name-line {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}

	/* A long name or username shortens rather than running off the page. */
	h1 {
		min-width: 0;
		margin: 0;
		font-size: clamp(1.6rem, 4.4vw, 2.6rem);
		letter-spacing: -0.02em;
		line-height: 1.1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.handle {
		display: flex;
		gap: 6px;
		margin: 6px 0 0;
		font-size: 0.9rem;
		min-width: 0;
	}

	.at {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}

	.handle span:last-child {
		flex: none;
	}

	.taste {
		margin-top: 6px;
		font-size: 0.9rem;
		color: var(--text-dim);
	}

	.taste strong {
		color: var(--accent);
		font-weight: 700;
	}

	.edit {
		flex: none;
		margin-bottom: 8px;
	}

	.bio,
	.private {
		max-width: 1440px;
		margin-left: auto;
		margin-right: auto;
		padding: 0 var(--page-pad);
	}

	.bio {
		margin-top: 16px;
		white-space: pre-line;
		line-height: 1.6;
		color: var(--text-dim);
	}

	.bio,
	.private {
		box-sizing: content-box;
		max-width: 68ch;
		margin-left: max(0px, calc((100% - 1440px) / 2));
	}

	.private {
		margin-top: 12px;
		font-size: 0.86rem;
	}

	.private a {
		color: var(--accent);
	}

	/* The shared segmented control, on the page's own column. */
	.tabs {
		margin: 22px 0 0 max(var(--page-pad), calc((100% - 1440px) / 2 + var(--page-pad)));
		max-width: calc(100% - 2 * var(--page-pad));
	}

	/* A phone: the picture and name centred under the banner, large enough
	   to be the page's subject. */
	@media (max-width: 600px) {
		.identity {
			flex-direction: column;
			align-items: center;
			text-align: center;
			gap: 10px;
			margin-top: -72px;
		}

		.names {
			width: 100%;
		}

		.handle,
		.name-line {
			justify-content: center;
		}

		.edit {
			margin: 0;
		}

		.bio,
		.private {
			margin-left: auto;
			text-align: center;
		}

		.tabs {
			width: auto;
			margin-inline: var(--page-pad);
		}

		.tabs a {
			flex: 1;
			padding-inline: 6px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.hero,
		.banner img {
			animation: none;
		}
	}
	/* Opened whole, over everything. */
	.whole {
		position: fixed;
		inset: 0;
		z-index: 400;
		display: grid;
		place-items: center;
		padding: 16px;
		background: rgb(0 0 0 / 0.86);
		cursor: zoom-out;
		animation: whole-in 180ms ease backwards;
	}

	.whole img {
		max-width: 100%;
		max-height: 100%;
		object-fit: contain;
		border-radius: 6px;
	}

	@keyframes whole-in {
		from {
			opacity: 0;
		}
	}

	/* Vertical, on a phone or a narrow window: the same picture as a tall
	   banner across the top, everything else under it. */
	.vertical .banner {
		height: min(125vw, 72dvh);
	}

	/* Vertical on a wide screen: the banner stands as a portrait column on
	   the left, with who it is at its foot, and stays in view while the
	   rest scrolls. ProfileShell's grid places the column and the tabs. */
	@media (min-width: 900px) {
		.hero.vertical {
			display: contents;
		}

		.vertical .side {
			grid-area: side;
			/* Held where the page puts it, so it never moves at all. */
			position: sticky;
			top: calc(var(--header-h) + 24px);
			max-height: calc(100dvh - var(--header-h) - 40px);
			overflow-y: auto;
			scrollbar-width: none;
			padding-bottom: 8px;
		}

		.vertical .banner {
			height: auto;
			aspect-ratio: 3 / 4;
			border-radius: 22px;
		}

		.vertical .identity {
			flex-direction: column;
			align-items: flex-start;
			gap: 10px;
			max-width: none;
			margin: -112px 0 0;
			padding: 0 18px;
		}

		.vertical .names {
			width: 100%;
			padding-bottom: 0;
		}

		.vertical h1 {
			font-size: clamp(1.5rem, 2.2vw, 2.1rem);
		}

		.vertical .edit {
			margin: 0;
		}

		.vertical .bio,
		.vertical .private {
			max-width: none;
			margin: 14px 0 0;
			padding: 0 18px;
		}

		.vertical .tabs {
			grid-area: tabs;
			margin: 0 0 22px;
			max-width: 100%;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.whole {
			animation: none;
		}
	}
</style>
