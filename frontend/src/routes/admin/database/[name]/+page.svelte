<script lang="ts">
	import { page } from '$app/state';
	import { api, ApiError } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { fileSize } from '#lib/format';
	import type { DbCell, DbColumn, DbPage } from '#lib/types';

	/** Characters a cell shows before it needs opening. */
	const SHORT = 60;
	const PER_PAGE = 50;

	const name = $derived(page.params.name ?? '');

	let data = $state<DbPage | null>(null);
	let loading = $state(true);
	let problem = $state<'denied' | 'missing' | 'failed' | null>(null);
	let at = $state(1);
	let sort = $state<string | undefined>(undefined);
	let dir = $state<'asc' | 'desc'>('asc');
	let typed = $state('');
	let term = $state('');
	/** Cells opened in full, as "row:column". */
	let open = $state<Record<string, boolean>>({});
	let token = 0;

	// What is typed is asked for once typing settles.
	$effect(() => {
		const now = typed;
		const wait = setTimeout(() => {
			if (now !== term) {
				term = now;
				at = 1;
			}
		}, 250);
		return () => clearTimeout(wait);
	});

	$effect(() => {
		void load(name, at, sort, dir, term);
	});

	async function load(
		table: string,
		pageAt: number,
		by: string | undefined,
		way: string,
		q: string
	) {
		const mine = ++token;
		loading = true;
		try {
			const answer = await api.dbTable(table, {
				page: pageAt,
				per_page: PER_PAGE,
				sort: by,
				dir: by ? (way as 'asc' | 'desc') : undefined,
				q: q.trim() || undefined
			});
			if (mine !== token) return;
			data = answer;
			open = {};
			problem = null;
		} catch (e) {
			if (mine !== token) return;
			const status = e instanceof ApiError ? e.status : 0;
			problem = status === 403 ? 'denied' : status === 404 ? 'missing' : 'failed';
		} finally {
			if (mine === token) loading = false;
		}
	}

	function sortBy(column: DbColumn) {
		if (column.hidden) return;
		if (sort === column.name) dir = dir === 'asc' ? 'desc' : 'asc';
		else {
			sort = column.name;
			dir = 'asc';
		}
		at = 1;
	}

	const pages = $derived(data ? Math.max(1, Math.ceil(data.total / data.per_page)) : 1);

	/** JSON written out with indents, when a text holds an object or a list. */
	function pretty(text: string): string | null {
		const first = text.trimStart()[0];
		if (first !== '{' && first !== '[') return null;
		try {
			return JSON.stringify(JSON.parse(text), null, 2);
		} catch {
			return null;
		}
	}

	const isBytes = (cell: DbCell): cell is { bytes: number } =>
		typeof cell === 'object' && cell !== null && 'bytes' in cell;
</script>

<svelte:head><title>{name} · Frog</title></svelte:head>

<div class="page">
	<header class="page-head">
		<div>
			<h1 class="mono">{name}</h1>
			<p class="sub">
				<a href="/admin/database">Database</a>
				{#if data}· {data.total.toLocaleString()} {data.total === 1 ? 'row' : 'rows'}{/if}
			</p>
		</div>
	</header>

	{#if problem === 'denied'}
		<Empty
			art="none"
			title="This page is for administrators"
			hint="Ask whoever runs this instance if you need it."
		/>
	{:else if problem === 'missing'}
		<Empty art="none" title="There is no table by that name">
			<a class="btn" href="/admin/database">All tables</a>
		</Empty>
	{:else if problem === 'failed' && !data}
		<Empty art="none" title="This table could not be read" hint="Try again in a minute." />
	{:else}
		<div class="tools">
			<input
				class="input search"
				type="search"
				placeholder="Search this table"
				aria-label="Search this table"
				bind:value={typed}
			/>
			{#if loading && data}<Spinner label="Reading" />{/if}
		</div>

		{#if !data}
			<Spinner label="Reading the table" />
		{:else}
			<div class="scroller card" class:dim={loading}>
				<table>
					<thead>
						<tr>
							{#each data.columns as column (column.name)}
								<th
									scope="col"
									aria-sort={sort === column.name
										? dir === 'asc'
											? 'ascending'
											: 'descending'
										: undefined}
								>
									{#if column.hidden}
										<span
											class="head"
											title={column.hidden === 'secret'
												? 'Kept on the server'
												: 'Shown as its size'}
										>
											{column.name}
											<Icon name={column.hidden === 'secret' ? 'shield' : 'database'} size={12} />
										</span>
									{:else}
										<button
											class="head sortable"
											class:on={sort === column.name}
											class:down={sort === column.name && dir === 'desc'}
											onclick={() => sortBy(column)}
										>
											{column.name}{#if column.primary_key}<span class="key">key</span>{/if}
										</button>
									{/if}
								</th>
							{/each}
						</tr>
					</thead>
					<tbody>
						{#each data.rows as row, r (r)}
							<tr>
								{#each row as cell, c (c)}
									{@const column = data.columns[c]}
									{@const id = `${r}:${c}`}
									<td>
										{#if column.hidden === 'secret'}
											{#if cell === null}
												<span class="null">NULL</span>
											{:else}
												<span class="hidden"><Icon name="shield" size={12} /> Hidden</span>
											{/if}
										{:else if cell === null}
											<span class="null">NULL</span>
										{:else if column.hidden === 'size' && typeof cell === 'number'}
											<span class="size">{fileSize(cell)}</span>
										{:else if isBytes(cell)}
											<span class="size">{fileSize(cell.bytes)}</span>
										{:else if typeof cell === 'string' && cell.length > SHORT}
											{#if open[id]}
												{@const json = pretty(cell)}
												<button class="long open" onclick={() => (open[id] = false)}>
													{#if json}<pre>{json}</pre>{:else}<span class="whole">{cell}</span>{/if}
												</button>
											{:else}
												<button class="long" title="Show all" onclick={() => (open[id] = true)}
													>{cell.slice(0, SHORT)}<span class="more">…</span></button
												>
											{/if}
										{:else}
											<span class:num={typeof cell === 'number'}>{String(cell)}</span>
										{/if}
									</td>
								{/each}
							</tr>
						{:else}
							<tr>
								<td class="none" colspan={data.columns.length}>
									{term ? 'Nothing matches.' : 'This table is empty.'}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			{#if pages > 1}
				<div class="pager">
					<button class="btn" disabled={at <= 1 || loading} onclick={() => (at -= 1)}
						>Previous</button
					>
					<span class="faint">Page {at} of {pages.toLocaleString()}</span>
					<button class="btn" disabled={at >= pages || loading} onclick={() => (at += 1)}
						>Next</button
					>
				</div>
			{/if}
		{/if}
	{/if}
</div>

<style>
	.mono,
	table {
		font-family: var(--font-mono, ui-monospace, monospace);
	}

	h1.mono {
		font-size: clamp(1.8rem, 4vw, 2.6rem);
		overflow-wrap: anywhere;
	}

	.sub a {
		color: var(--text);
	}

	.tools {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 14px;
	}

	.search {
		max-width: 340px;
	}

	/* Wide tables scroll sideways inside their pane, never the page. */
	.scroller {
		overflow-x: auto;
		transition: opacity var(--fast) var(--ease);
	}

	.scroller.dim {
		opacity: 0.6;
	}

	table {
		border-collapse: collapse;
		min-width: 100%;
		font-size: 0.8rem;
	}

	th,
	td {
		padding: 8px 12px;
		text-align: left;
		vertical-align: top;
		white-space: nowrap;
		overflow-wrap: normal;
		border-bottom: 1px solid var(--line);
	}

	tbody tr:last-child td {
		border-bottom: 0;
	}

	th {
		position: sticky;
		top: 0;
		background: var(--bg-elev-2);
		font-weight: 600;
		color: var(--text-dim);
	}

	.head {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font: inherit;
		color: inherit;
	}

	.sortable {
		background: none;
		border: 0;
		padding: 0;
		cursor: pointer;
	}

	.sortable:hover,
	.sortable.on {
		color: var(--text);
	}

	/* A small triangle drawn in CSS shows which way a column is sorted. */
	.sortable.on::after {
		content: '';
		border: 4px solid transparent;
		border-bottom-color: currentColor;
		border-top-width: 0;
	}

	.sortable.on.down::after {
		border-bottom-color: transparent;
		border-top: 4px solid currentColor;
		border-bottom-width: 0;
	}

	.key {
		font-size: 0.7rem;
		color: var(--accent);
	}

	.null {
		color: var(--text-faint, var(--text-dim));
		opacity: 0.6;
	}

	.hidden {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		color: var(--text-dim);
	}

	.size,
	.num {
		font-variant-numeric: tabular-nums;
	}

	.size {
		color: var(--text-dim);
	}

	.long {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}

	.more {
		color: var(--accent);
	}

	.long.open {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		max-width: min(70ch, 80vw);
		display: block;
	}

	.long.open pre {
		margin: 0;
		font: inherit;
		white-space: pre-wrap;
	}

	.none {
		color: var(--text-dim);
		padding: 18px 12px;
	}

	.pager {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 14px;
		margin-top: 16px;
	}

	@media (max-width: 560px) {
		.search {
			max-width: none;
		}
	}
</style>
