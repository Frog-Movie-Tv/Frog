<script lang="ts">
	import { onMount } from 'svelte';
	import { api, ApiError } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { fileSize } from '#lib/format';
	import type { DbTable } from '#lib/types';

	let tables = $state<DbTable[]>([]);
	let loading = $state(true);
	let denied = $state(false);
	let failed = $state(false);

	onMount(async () => {
		try {
			tables = (await api.dbTables()).tables;
		} catch (e) {
			denied = e instanceof ApiError && e.status === 403;
			failed = !denied;
		} finally {
			loading = false;
		}
	});

	const rows = $derived(tables.reduce((n, t) => n + t.rows, 0));
</script>

<svelte:head><title>Database · Frog</title></svelte:head>

<div class="page">
	<header class="page-head">
		<div>
			<h1>Database</h1>
			<p class="sub">
				<a href="/admin">Administration</a> · Every table, read only. Secrets stay on the server.
			</p>
		</div>
	</header>

	{#if loading}
		<Spinner label="Reading the database" />
	{:else if denied}
		<Empty
			art="none"
			title="This page is for administrators"
			hint="Ask whoever runs this instance if you need it."
		/>
	{:else if failed}
		<Empty art="none" title="The database could not be read" hint="Try again in a minute." />
	{:else}
		<p class="faint count">
			{tables.length} tables · {rows.toLocaleString()} rows
		</p>
		<nav class="tables list-pane" aria-label="Tables">
			{#each tables as table (table.name)}
				<a class="table hit" href="/admin/database/{encodeURIComponent(table.name)}">
					<span class="name">{table.name}</span>
					<span class="rows">{table.rows.toLocaleString()} {table.rows === 1 ? 'row' : 'rows'}</span
					>
					<span class="bytes faint">{table.bytes === null ? '' : fileSize(table.bytes)}</span>
				</a>
			{/each}
		</nav>
	{/if}
</div>

<style>
	.sub a {
		color: var(--text);
	}

	.count {
		margin-bottom: 12px;
		font-size: 0.9rem;
	}

	.table {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto 72px;
		align-items: center;
		gap: 16px;
		padding: 12px 18px;
		color: var(--text);
		text-decoration: none;
		transition: background var(--fast) var(--ease);
	}

	.table:hover {
		background: var(--bg-elev-2);
	}

	.name {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.9rem;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		overflow-wrap: normal;
	}

	.rows,
	.bytes {
		font-variant-numeric: tabular-nums;
		text-align: right;
		white-space: nowrap;
		overflow-wrap: normal;
		font-size: 0.9rem;
	}

	@media (max-width: 560px) {
		.table {
			grid-template-columns: minmax(0, 1fr) auto;
			padding: 12px 14px;
		}
		.bytes {
			display: none;
		}
	}
</style>
