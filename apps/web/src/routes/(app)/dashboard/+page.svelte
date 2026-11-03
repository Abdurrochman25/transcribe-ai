<script lang="ts">
	import { listTranscripts, cancelTranscript, retryTranscript, type Transcript } from '../../../lib/api/transcripts';
	import { subscribeJob, type JobHandle } from '../../../lib/api/sse';
	import { formatBytes, formatDuration, formatRelative } from '../../../lib/utils/format';
	import { sampleTranscripts } from '../../../lib/dev/sample';

	let items = $state<Transcript[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let search = $state('');
	let tab = $state<'semua' | 'berjalan' | 'selesai' | 'gagal'>('semua');

	/** DEV-only: dataset sampel untuk inspeksi desain (?preview&sample). */
	const sample = import.meta.env.DEV && new URLSearchParams(window.location.search).has('sample');
	const preview = import.meta.env.DEV && new URLSearchParams(window.location.search).has('preview');

	// Overlay progres live (SSE) untuk item aktif.
	let live = $state<Record<string, { status: string; progress: number }>>({});
	const handles = new Map<string, JobHandle>();

	const RUNNING = new Set(['uploaded', 'queued', 'validating', 'chunking', 'transcribing', 'processing']);

	const tabOf = (t: Transcript): 'berjalan' | 'selesai' | 'gagal' | 'lain' =>
		RUNNING.has(t.status) ? 'berjalan'
		: t.status === 'completed' ? 'selesai'
		: t.status === 'failed' ? 'gagal'
		: 'lain';

	const displayStatus = (t: Transcript) => live[t.id]?.status ?? t.status;
	const displayProgress = (t: Transcript) => live[t.id]?.progress ?? t.progress;

	const effective = $derived(
		items.map((t) => ({ ...t, status: displayStatus(t) as Transcript['status'], progress: displayProgress(t) }))
	);
	const counts = $derived.by(() => {
		const c = { semua: effective.length, berjalan: 0, selesai: 0, gagal: 0, lain: 0 };
		for (const t of effective) c[tabOf(t)] += 1;
		return c;
	});
	const visible = $derived(tab === 'semua' ? effective : effective.filter((t) => tabOf(t) === tab));

	/** Baris pertuturan layar: temuan dari data, dihitung dari baris. */
	const finding = $derived.by(() => {
		if (loading) return '';
		if (effective.length === 0) return 'Belum ada transkripsi — unggah rekaman pertamamu.';
		const menit = Math.round(
			effective.filter((t) => t.status === 'completed').reduce((a, t) => a + (t.duration_seconds ?? 0), 0) / 60
		);
		const bagian = [`${counts.selesai} selesai`];
		if (counts.berjalan > 0) bagian.push(`${counts.berjalan} sedang diproses`);
		if (counts.gagal > 0) bagian.push(`${counts.gagal} gagal`);
		if (menit > 0) bagian.push(`${menit} menit tersedia`);
		return bagian.join(' · ') + '.';
	});

	const statusPill = (status: string) =>
		({
			completed: 'bg-pill-positive-bg text-pill-positive-ink',
			failed: 'bg-pill-negative-bg text-pill-negative-ink',
			cancelled: 'bg-pill-warning-bg text-pill-warning-ink',
			queued: 'bg-pill-neutral-bg text-pill-neutral-ink',
			uploaded: 'bg-pill-neutral-bg text-pill-neutral-ink'
		})[status] ?? 'bg-pill-neutral-bg text-pill-neutral-ink';

	const meta = (t: Transcript) =>
		[t.source_filename, formatBytes(t.size_bytes), t.language ? `bahasa ${t.language}` : null, formatRelative(t.created_at)]
			.filter(Boolean)
			.join(' · ');

	const hrefOf = (t: Transcript) => (preview ? `/t/${t.id}?preview=1&sample=1` : `/t/${t.id}`);

	async function load() {
		loading = true;
		error = null;
		if (sample) {
			items = sampleTranscripts as unknown as Transcript[];
			total = sampleTranscripts.length;
			loading = false;
			return;
		}
		if (preview) {
			// Mode inspeksi tanpa API: kondisi first-run (akun baru).
			items = [];
			total = 0;
			loading = false;
			return;
		}
		try {
			const list = await listTranscripts(search ? { q: search } : {});
			items = list.items;
			total = list.total;
			watchActive();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Gagal memuat riwayat.';
		} finally {
			loading = false;
		}
	}

	/** Subscribe SSE untuk item aktif; final → muat ulang daftar (docs/02 §6). */
	function watchActive() {
		for (const t of items) {
			const active = RUNNING.has(t.status) || RUNNING.has(live[t.id]?.status ?? '');
			if (active && !handles.has(t.id)) {
				live[t.id] = { status: t.status, progress: t.progress };
				handles.set(
					t.id,
					subscribeJob(
						t.id,
						(ev) => {
							if (ev.status) live[t.id] = { status: ev.status, progress: ev.progress ?? 0 };
						},
						() => {
							handles.delete(t.id);
							load();
						}
					)
				);
			}
		}
	}

	async function onCancel(id: string) {
		try {
			await cancelTranscript(id);
			load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Gagal membatalkan.';
		}
	}

	async function onRetry(id: string) {
		try {
			await retryTranscript(id);
			load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Gagal mengulang job.';
		}
	}

	$effect(() => {
		const q = search;
		const t = setTimeout(() => load(), q ? 300 : 0);
		return () => clearTimeout(t);
	});
</script>

<div class="mx-auto max-w-6xl px-6 py-8">
	<!-- Baris judul: temuan di bawah judul, satu aksi primer -->
	<div class="flex flex-wrap items-end justify-between gap-4">
		<div>
			<h1 class="text-2xl font-semibold tracking-tight">Transkripsi</h1>
			<p class="mt-1 text-sm text-muted" aria-live="polite">{finding}</p>
		</div>
		<a
			href={preview ? '/upload?preview=1' : '/upload'}
			class="h-9 rounded-[var(--radius-control)] bg-accent px-4 text-sm font-medium leading-9 text-white transition-colors hover:bg-accent-hover"
		>
			Upload audio
		</a>
	</div>

	<!-- Tab keadaan di atas garis penuh -->
	{#if !loading && counts.semua > 0}
		<div class="mt-6 border-b border-line">
			<div class="flex gap-6 overflow-x-auto">
				{#each [['semua', 'Semua'], ['berjalan', 'Berjalan'], ['selesai', 'Selesai'], ['gagal', 'Gagal']] as [key, label] }
					<button
						type="button"
						onclick={() => (tab = key as typeof tab)}
						class="relative whitespace-nowrap pb-2.5 text-sm {tab === key
							? 'font-medium text-text'
							: 'text-muted transition-colors hover:text-text'}"
						aria-current={tab === key ? 'true' : undefined}
					>
						{label}
						<span class="num ml-1.5 text-xs {tab === key ? 'text-muted' : 'text-faint'}">
							{counts[key as keyof typeof counts]}
						</span>
						{#if tab === key}
							<span class="absolute inset-x-0 bottom-[-1px] h-0.5 bg-text" aria-hidden="true"></span>
						{/if}
					</button>
				{/each}
			</div>
		</div>

		<!-- Toolbar: cari kiri, hitungan kanan -->
		<div class="mt-5 flex items-center justify-between gap-4">
			<input
				type="search"
				bind:value={search}
				placeholder="Cari judul…"
				aria-label="Cari transkripsi"
				class="h-8 w-72 max-w-full rounded-[var(--radius-control)] border border-control bg-surface px-3 text-sm outline-hidden placeholder:text-faint focus:border-muted"
			/>
			<p class="num hidden text-xs text-muted sm:block">
				{visible.length} dari {total} transkripsi
			</p>
		</div>
	{/if}

	<div class="mt-4">
		{#if error}
			<div class="rounded-[var(--radius-card)] border border-line bg-surface px-4 py-3">
				<p class="text-sm text-danger">{error}</p>
				<button type="button" onclick={load} class="mt-1 text-sm font-medium underline">Coba lagi</button>
			</div>
		{:else if loading}
			<!-- Loading: layout akhir, batang abu datar seukuran isi (tanpa shimmer) -->
			<div class="divide-y divide-rule rounded-[var(--radius-card)] border border-line bg-surface" aria-label="Memuat">
				{#each Array(4) as _}
					<div class="px-5 py-4">
						<div class="h-4 w-1/3 rounded-sm bg-surface-2"></div>
						<div class="mt-2.5 h-3 w-1/2 rounded-sm bg-surface-2"></div>
					</div>
				{/each}
			</div>
		{:else if items.length === 0}
			<!-- First run: bingkai + aksi + cara kerja; ruang di bawahnya satu istirahat (§11) -->
			<div class="rounded-[var(--radius-card)] border border-dashed border-control bg-surface px-6 py-10 text-center">
				<p class="text-sm text-muted">
					{search
						? `Tidak ada transkripsi bernama “${search}”.`
						: 'Rekaman pertamamu akan tercantum di sini.'}
				</p>
				{#if !search}
					<ol class="mx-auto mt-6 grid max-w-lg gap-2.5 text-left text-sm text-muted sm:grid-cols-3">
						<li class="rounded-[var(--radius-control)] bg-surface-2/60 px-4 py-3">
							<span class="font-medium text-text">Unggah</span> rekaman kuliah, rapat, atau wawancara.
						</li>
						<li class="rounded-[var(--radius-control)] bg-surface-2/60 px-4 py-3">
							<span class="font-medium text-text">Ditranskripsi</span> otomatis dengan Whisper — pantau progresnya live.
						</li>
						<li class="rounded-[var(--radius-control)] bg-surface-2/60 px-4 py-3">
							<span class="font-medium text-text">Perbaiki &amp; ekspor</span> teks per segment, unduh SRT atau TXT.
						</li>
					</ol>
					<a
						href={preview ? '/upload?preview=1' : '/upload'}
						class="mt-6 inline-block h-9 rounded-[var(--radius-control)] bg-accent px-4 text-sm font-medium leading-9 text-white transition-colors hover:bg-accent-hover"
					>
						Upload sekarang
					</a>
				{/if}
			</div>
		{:else if visible.length === 0}
			<div class="rounded-[var(--radius-card)] border border-dashed border-control bg-surface px-6 py-10 text-center">
				<p class="text-sm text-muted">Tidak ada transkripsi {tab} saat ini.</p>
			</div>
		{:else}
			<!-- Panel ledger: baris dua-garis, angka rata kanan tabular, satu tanda status, satu aksi -->
			<ul class="divide-y divide-rule rounded-[var(--radius-card)] border border-line bg-surface">
				{#each visible as t (t.id)}
					{@const status = displayStatus(t)}
					{@const running = RUNNING.has(status)}
					{@const done = status === 'completed'}
					{@const failed = status === 'failed'}
					<li class="px-5 py-3.5 transition-colors hover:bg-surface-2/50">
						<div class="flex items-baseline gap-4">
							<div class="min-w-0 flex-1">
								{#if done}
									<a href={hrefOf(t)} class="block truncate text-sm font-medium hover:underline">{t.title}</a>
								{:else}
									<p class="truncate text-sm font-medium">{t.title}</p>
								{/if}
								<p class="mt-1 truncate text-[13px] text-muted">{meta(t)}</p>
							</div>

							<!-- angka: durasi (mono, rata kanan) — tanda seru saat belum diketahui -->
							<span class="num hidden w-20 shrink-0 text-right text-[13px] text-muted sm:block">
								{t.duration_seconds != null ? formatDuration(t.duration_seconds) : '—'}
							</span>

							<span
								class="flex h-6 shrink-0 items-center whitespace-nowrap rounded-full px-2.5 text-xs font-medium {statusPill(status)}"
							>
								{status === 'completed'
									? 'Selesai'
									: status === 'failed'
										? 'Gagal'
										: status === 'cancelled'
											? 'Dibatalkan'
											: running
												? 'Diproses'
												: status}
							</span>

							<!-- satu aksi tetap di kolom belakang, nadanya sama tiap baris -->
							<span class="w-20 shrink-0 text-right">
								{#if running}
									<button type="button" onclick={() => onCancel(t.id)} class="text-[13px] text-muted hover:text-text hover:underline">
										Batalkan
									</button>
								{:else if failed}
									<button type="button" onclick={() => onRetry(t.id)} class="text-[13px] font-medium hover:underline">
										Ulangi
									</button>
								{:else if done}
									<a href={hrefOf(t)} class="text-[13px] font-medium hover:underline">Lihat</a>
								{/if}
							</span>
						</div>

						{#if running && displayProgress(t) > 0}
							<!-- satu hue data: progres live -->
							<div class="mt-2.5 flex items-center gap-3">
								<div class="h-1 flex-1 overflow-hidden rounded-full bg-line" role="progressbar"
									aria-valuenow={displayProgress(t)} aria-valuemin="0" aria-valuemax="100">
									<div class="h-full rounded-full bg-data transition-[width] duration-300" style="width: {displayProgress(t)}%"></div>
								</div>
								<span class="num w-9 text-right text-xs text-muted">{displayProgress(t)}%</span>
							</div>
						{:else if failed && t.error}
							<p class="mt-1.5 truncate text-[13px] text-danger">{t.error}</p>
						{/if}
					</li>
				{/each}
			</ul>
			<p class="mt-3 text-xs text-muted sm:hidden">Durasi tampil setelah transkripsi selesai.</p>
		{/if}
	</div>
</div>
