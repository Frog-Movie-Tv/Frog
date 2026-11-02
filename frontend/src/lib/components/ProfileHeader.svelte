<script lang="ts" module>
	export type ProfileTab = 'overview' | 'library' | 'ratings' | 'compare';
</script>

<script lang="ts">
	import { img } from '#lib/api';
	import Avatar from './Avatar.svelte';
	import Icon from './Icon.svelte';
	import { prettyDate } from '#lib/format';
	import type { Profile } from '#lib/types';

	interface Props {
		profile: Profile;
		tab: ProfileTab;
	}

	let { profile, tab }: Props = $props();

	const banner = $derived(
		profile.banner ?? (profile.banner_backdrop ? img(profile.banner_backdrop, 'w1280') : null)
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

<header class="hero">
	<div class="banner" class:plain={!banner}>
		{#if banner}
			<img src={banner} alt="" style:object-position={profile.banner_position ?? undefined} />
		{/if}
		<div class="shade" aria-hidden="true"></div>
	</div>

	<div class="identity">
		<span class="picture">
			<Avatar src={profile.avatar} name={profile.username} size={112} />
		</span>
		<div class="names">
			<h1 title={shownName}>{shownName}</h1>
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

	<nav class="tabs" aria-label="Profile">
		{#each tabs as t (t.id)}
			<a href={t.href} class:on={tab === t.id} aria-current={tab === t.id ? 'page' : undefined}>
				{t.label}
			</a>
		{/each}
	</nav>
</header>

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

	.banner.plain {
		background:
			radial-gradient(circle at 20% 20%, var(--accent-soft), transparent 55%),
			linear-gradient(135deg, var(--bg-elev-2), var(--bg) 80%);
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

	.names {
		flex: 1;
		min-width: 0;
		padding-bottom: 6px;
	}

	/* A long name or username shortens rather than running off the page. */
	h1 {
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
	.private,
	.tabs {
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

	.tabs {
		display: flex;
		gap: 4px;
		margin-top: 22px;
		border-bottom: 1px solid var(--line);
		overflow-x: auto;
		scrollbar-width: none;
	}

	.tabs a {
		position: relative;
		flex: none;
		padding: 12px 14px;
		color: var(--text-dim);
		font-weight: 650;
		font-size: 0.92rem;
		text-decoration: none;
		transition: color var(--fast) var(--ease);
	}

	.tabs a:hover {
		color: var(--text);
	}

	.tabs a.on {
		color: var(--text);
	}

	.tabs a.on::after {
		content: '';
		position: absolute;
		left: 10px;
		right: 10px;
		bottom: -1px;
		height: 2px;
		border-radius: 2px;
		background: var(--accent);
		animation: grow 260ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	@keyframes grow {
		from {
			transform: scaleX(0.3);
			opacity: 0;
		}
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

		.handle {
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
			justify-content: space-between;
		}

		.tabs a {
			flex: 1;
			text-align: center;
			padding: 12px 6px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.hero,
		.banner img,
		.tabs a.on::after {
			animation: none;
		}
	}
</style>
