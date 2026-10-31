<script lang="ts">
	import { listTranscripts, type Transcript } from '../../../lib/api/transcripts';
	import { auth } from '../../../lib/stores/auth.svelte';
	import { formatBytes, formatDuration, formatRelative, STATUS_LABEL } from '../../../lib/utils/format';

	let items = $state<Transcript[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let search = $state('');

	const statusClass = (status: string) =>
		({
			completed: 'bg-success/10 text-success',
			failed: 'bg-danger/10 text-danger',
			cancelled: 'bg-warning/10 text-warning',
			queued: 'bg-surface-2 text-muted',
			uploaded: 'bg-surface-2 text-muted'
		})[status] ?? 'bg-accent-soft text-accent';

	async function load() {
		loading = true;
		error = null;
		try {
			const list = await listTranscripts(search ? { q: search } : {});
			items = list.items;
			total = list.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Gagal memuat riwayat.';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		// debounced search
		const q = search;
		const t = setTimeout(() => load(), q ? 300 : 0);
		return () => clearTimeout(t);
	});
</script>

<div class="mx-auto max-w-5xl px-6">
	<header class="flex items-center justify-between py-5">
		<a href="/dashboard" class="flex items-center gap-2 font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span>
			TranscribeAI
		</a>
		<div class="flex items-center gap-4">
			<span class="hidden text-sm text-muted sm:inline">{auth.user?.email}</span>
			<button type="button" onclick={() => auth.logout()}
				class="rounded-lg border border-line bg-surface px-4 py-2 text-sm font-medium transition-colors hover:bg-surface-2">
				Keluar
			</button>
		</div>
	</header>

	<main class="py-4">
		<div class="mb-6 flex flex-wrap items-center justify-between gap-3">
			<div>
				<h1 class="text-2xl font-bold tracking-tight">Halo, {auth.user?.name} 👋</h1>
				<p class="mt-1 text-sm text-muted">
					{total > 0 ? `${total} transkripsi tercatat` : 'Siap mentranskribasi audio pertamamu?'}
				</p>
			</div>
			<a href="/upload"
				class="rounded-lg bg-accent px-5 py-2.5 font-medium text-white transition-colors hover:bg-accent-hover">
				＋ Upload audio
			</a>
		</div>

		<div class="mb-4">
			<input type="search" bind:value={search} placeholder="Cari judul transkripsi…"
				aria-label="Cari transkripsi"
				class="w-full rounded-lg border border-line bg-surface px-4 py-2.5 text-sm outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30" />
		</div>

		{#if error}
			<p role="alert" class="rounded-lg bg-danger/10 px-4 py-3 text-sm text-danger">{error}</p>
		{:else if loading}
			<!-- skeleton -->
			<div class="space-y-3" aria-label="Memuat">
				{#each Array(4) as _}
					<div class="h-16 animate-pulse rounded-xl border border-line bg-surface"></div>
				{/each}
			</div>
		{:else if items.length === 0}
			<div class="rounded-xl border border-dashed border-line bg-surface p-12 text-center">
				<div class="mx-auto mb-4 flex size-12 items-center justify-center rounded-full bg-accent-soft text-2xl" aria-hidden="true">🎙️</div>
				<h2 class="font-semibold">{search ? 'Tidak ada hasil' : 'Belum ada transkripsi'}</h2>
				<p class="mx-auto mt-1 max-w-md text-sm text-muted">
					{search ? `Tidak ditemukan transkripsi dengan kata “${search}”.` : 'Unggah audio pertamamu — transkripsi akan muncul di sini.'}
				</p>
				{#if !search}
					<a href="/upload" class="mt-4 inline-block rounded-lg bg-accent px-5 py-2.5 text-sm font-medium text-white transition-colors hover:bg-accent-hover">Upload sekarang</a>
				{/if}
			</div>
		{:else}
			<ul class="space-y-3">
				{#each items as t (t.id)}
					<li class="rounded-xl border border-line bg-surface p-4 shadow-sm transition-colors hover:border-accent/40">
						<div class="flex flex-wrap items-center justify-between gap-2">
							<div class="min-w-0">
								<p class="truncate font-medium">{t.title}</p>
								<p class="mt-0.5 truncate text-xs text-muted">
									{t.source_filename} · {formatBytes(t.size_bytes)}
									{#if t.duration_seconds}· {formatDuration(t.duration_seconds)}{/if}
									· {formatRelative(t.created_at)}
								</p>
							</div>
							<div class="flex items-center gap-2">
								<span class="rounded-full px-3 py-1 text-xs font-medium {statusClass(t.status)}">
									{STATUS_LABEL[t.status] ?? t.status}
								</span>
								<!-- TODO(fase 4): link ke editor /t/{t.id} -->
								<span class="text-muted" aria-hidden="true">⋯</span>
							</div>
						</div>
						{#if t.status !== 'completed' && t.status !== 'failed' && t.status !== 'cancelled' && t.progress > 0}
							<div class="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-surface-2">
								<div class="h-full rounded-full bg-accent" style="width: {t.progress}%"></div>
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</main>
</div>
