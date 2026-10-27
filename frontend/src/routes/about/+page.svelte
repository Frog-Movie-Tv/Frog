<script lang="ts">
	import Icon from '#lib/components/Icon.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import { offline } from '#lib/offline.svelte';
	import { session } from '#lib/session.svelte';

	const authors = [
		{
			name: 'Mihai Constantin',
			role: 'Design and development',
			href: 'https://github.com/MihCons'
		},
		{
			name: 'Zjef Schaeken',
			role: 'Design and development',
			href: 'https://github.com/zschaeken'
		}
	];

	const credits = [
		{
			name: 'TMDB',
			role: 'Films, shows, artwork and episode data',
			href: 'https://www.themoviedb.org'
		},
		{ name: 'Lucide', role: 'Icons, ISC licence', href: 'https://lucide.dev' },
		{
			name: 'Inter',
			role: 'Interface type, SIL Open Font Licence',
			href: 'https://rsms.me/inter/'
		},
		{
			name: 'Outfit',
			role: 'Display type, SIL Open Font Licence',
			href: 'https://github.com/Outfitio/Outfit-Fonts'
		},
		{
			name: 'Newsreader',
			role: 'Serif option, SIL Open Font Licence',
			href: 'https://github.com/productiontype/Newsreader'
		},
		{
			name: 'JetBrains Mono',
			role: 'Monospace, SIL Open Font Licence',
			href: 'https://www.jetbrains.com/lp/mono/'
		},
		{ name: 'Svelte', role: 'Interface framework, MIT', href: 'https://svelte.dev' },
		{ name: 'Axum', role: 'HTTP server, MIT', href: 'https://github.com/tokio-rs/axum' },
		{ name: 'SQLx', role: 'Database access, MIT', href: 'https://github.com/launchbadge/sqlx' },
		{ name: 'SQLite', role: 'Storage, public domain', href: 'https://sqlite.org' },
		{
			name: 'fflate',
			role: 'Reading ZIP exports, MIT',
			href: 'https://github.com/101arrowz/fflate'
		}
	];
</script>

<svelte:head>
	<title>About · Frog</title>
</svelte:head>

<div class="page">
	<header class="head">
		<Logo size={56} />
		<div>
			<h1>Frog</h1>
			<p class="muted">Version {session.status?.version ?? '—'} · Ribbit</p>
		</div>
	</header>

	<p class="lede">
		A private film and television diary that runs on your own machine. Your library is one file on
		your own disk, and nothing about it leaves the house.
	</p>

	<section class="block">
		<h2>This instance</h2>
		<dl class="facts card">
			<div>
				<dt>Signed in as</dt>
				<dd>{session.status?.username ?? '—'}</dd>
			</div>
			<div>
				<dt>Version</dt>
				<dd>{session.status?.version ?? '—'}</dd>
			</div>
			<div>
				<dt>Region</dt>
				<dd>{session.status?.region ?? '—'}</dd>
			</div>
			<div>
				<dt>Metadata</dt>
				<dd class={session.status?.tmdb_configured ? 'ok' : 'bad'}>
					{session.status?.tmdb_configured ? 'Connected' : 'Not configured'}
				</dd>
			</div>
			<div>
				<dt>Offline mode</dt>
				<dd class={offline.ready ? 'ok' : 'warn'}>
					{offline.state.status === 'ready'
						? 'Active'
						: offline.state.status === 'pending'
							? 'Starting'
							: 'Unavailable'}
				</dd>
			</div>
		</dl>

		{#if offline.state.status === 'unavailable'}
			<p class="notice">
				<Icon name="info" size={16} />
				<span>
					{offline.state.reason}
					{#if offline.state.detail}<br /><span class="faint">{offline.state.detail}</span>{/if}
				</span>
			</p>
		{/if}
	</section>

	<section class="block">
		<h2>Data</h2>
		<p class="muted">
			Films, shows, artwork and episode listings come from
			<a href="https://www.themoviedb.org" target="_blank" rel="noreferrer noopener">TMDB</a>
			and are cached locally. This product uses the TMDB API but is not endorsed or certified by TMDB.
		</p>
		<a class="tmdb" href="https://www.themoviedb.org" target="_blank" rel="noreferrer noopener">
			<Icon name="external" size={14} />
			themoviedb.org
		</a>
	</section>

	<section class="block">
		<h2>Made by</h2>
		<ul class="credits">
			{#each authors as author (author.name)}
				<li>
					<a href={author.href} target="_blank" rel="noreferrer noopener">
						<strong>{author.name}</strong>
						<span class="faint">{author.role}</span>
						<Icon name="external" size={13} />
					</a>
				</li>
			{/each}
		</ul>
	</section>

	<section class="block">
		<h2>Built with</h2>
		<ul class="credits">
			{#each credits as credit (credit.name)}
				<li>
					<a href={credit.href} target="_blank" rel="noreferrer noopener">
						<strong>{credit.name}</strong>
						<span class="faint">{credit.role}</span>
						<Icon name="external" size={13} />
					</a>
				</li>
			{/each}
		</ul>
	</section>

	<section class="block">
		<h2>Licence</h2>
		<p class="muted">
			Frog is open source under the MIT licence. The frog is original artwork, drawn from geometry
			that lives in the repository.
		</p>
		<div class="actions">
			<a class="btn" href="/whats-new">Release notes</a>
			<a
				class="btn ghost"
				href="https://github.com/Frog-Movie-Tv/Frog"
				target="_blank"
				rel="noreferrer noopener"
			>
				<Icon name="external" size={15} />
				Source
			</a>
		</div>
	</section>
</div>

<style>
	.head {
		display: flex;
		align-items: center;
		gap: 20px;
		margin-bottom: 24px;
	}

	.head p {
		margin-top: 4px;
		font-size: 0.88rem;
	}

	.lede {
		max-width: 62ch;
		font-size: 1.05rem;
		color: var(--text-dim);
		margin-bottom: 40px;
	}

	.block {
		margin-bottom: 40px;
		max-width: 70ch;
	}

	.block h2 {
		margin-bottom: 14px;
	}

	.block p {
		font-size: 0.92rem;
	}

	.facts {
		margin: 0;
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
		font-weight: 600;
	}

	dd.ok {
		color: var(--good);
	}
	dd.warn {
		color: var(--warn);
	}
	dd.bad {
		color: var(--bad);
	}

	.notice {
		display: flex;
		gap: 10px;
		margin-top: 12px;
		padding: 12px 14px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev);
		border: 1px solid var(--line);
		font-size: 0.86rem;
		line-height: 1.5;
	}

	.tmdb {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		margin-top: 12px;
		font-size: 0.86rem;
		color: var(--accent);
	}

	.tmdb:hover {
		text-decoration: underline;
	}

	.credits {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 8px;
	}

	.credits a {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 11px 14px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		font-size: 0.86rem;
	}

	.credits a:hover {
		border-color: var(--line-strong);
	}

	.credits strong {
		font-weight: 600;
	}

	.credits .faint {
		flex: 1;
		font-size: 0.76rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		margin-top: 16px;
	}

	@media (max-width: 720px) {
		.head {
			gap: 14px;
		}
	}
</style>
