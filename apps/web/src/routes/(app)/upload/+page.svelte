<script lang="ts">
	import { goto } from '$app/navigation';
	import { uploadTranscript, type Transcript } from '../../../lib/api/transcripts';
	import { ApiError } from '../../../lib/api/types';

	const ALLOWED_EXT = ['mp3', 'wav', 'm4a', 'mp4', 'ogg', 'oga', 'opus', 'flac', 'webm'];
	const MAX_BYTES = 200 * 1024 * 1024;

	let file = $state<File | null>(null);
	let title = $state('');
	let language = $state(''); // '' = auto-detect
	let model = $state('whisper-large-v3-turbo');
	let prompt = $state('');

	let dragging = $state(false);
	let uploading = $state(false);
	let progress = $state(0);
	let error = $state<string | null>(null);
	let done = $state<Transcript | null>(null);

	const fileLabel = $derived(
		file ? `${file.name} · ${(file.size / 1024 / 1024).toFixed(1)} MB` : null
	);

	function pick(f: File | undefined | null) {
		error = null;
		if (!f) return;
		const ext = f.name.split('.').pop()?.toLowerCase() ?? '';
		if (!ALLOWED_EXT.includes(ext)) {
			error = `Format .${ext} tidak didukung. Gunakan: ${ALLOWED_EXT.join(', ')}.`;
			return;
		}
		if (f.size > MAX_BYTES) {
			error = 'Ukuran file melebihi 200 MB.';
			return;
		}
		file = f;
		if (!title) title = f.name.replace(/\.[^.]+$/, '');
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		dragging = false;
		pick(e.dataTransfer?.files?.[0]);
	}

	function onFileInput(e: Event) {
		const input = e.target as HTMLInputElement;
		pick(input.files?.[0]);
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!file || uploading) return;
		error = null;
		uploading = true;
		progress = 0;
		try {
			done = await uploadTranscript({
				file,
				title: title.trim() || undefined,
				language: language || undefined,
				model,
				prompt: prompt.trim() || undefined,
				onProgress: (f) => (progress = f)
			});
		} catch (err) {
			if (err instanceof ApiError) {
				const detail = err.details?.map((d) => `${d.field}: ${d.issue}`).join(' · ');
				error = detail ? `${err.message} (${detail})` : err.message;
			} else {
				error = 'Tidak bisa menghubungi server. Coba lagi.';
			}
		} finally {
			uploading = false;
		}
	}

	function reset() {
		file = null;
		title = '';
		prompt = '';
		progress = 0;
		done = null;
	}
</script>

<div class="mx-auto max-w-2xl px-6">
	<header class="py-5">
		<a href="/dashboard" class="text-sm text-muted transition-colors hover:text-text">← Kembali</a>
		<h1 class="mt-2 text-2xl font-bold tracking-tight">Transkripsi Baru</h1>
	</header>

	{#if done}
		<!-- Sukses -->
		<div class="rounded-xl border border-success/30 bg-success/5 p-8 text-center">
			<div class="mx-auto mb-3 flex size-12 items-center justify-center rounded-full bg-success/15 text-2xl">✅</div>
			<h2 class="text-lg font-semibold">{done.title}</h2>
			<p class="mt-1 text-sm text-muted">
				Terkirim ke antrean (status: {done.status}). Transkripsi berjalan di Fase 3 — riwayat sudah
				tercatat di dashboard.
			</p>
			<div class="mt-5 flex justify-center gap-3">
				<a href="/dashboard" class="rounded-lg bg-accent px-5 py-2.5 font-medium text-white transition-colors hover:bg-accent-hover">
					Lihat dashboard
				</a>
				<button type="button" onclick={reset} class="rounded-lg border border-line bg-surface px-5 py-2.5 font-medium transition-colors hover:bg-surface-2">
					Upload lagi
				</button>
			</div>
		</div>
	{:else}
		<form onsubmit={submit} class="space-y-5">
			<!-- Dropzone -->
			<div
				role="button"
				tabindex="0"
				aria-label="Pilih atau tarik file audio ke sini"
				class="flex cursor-pointer flex-col items-center justify-center rounded-xl border-2 border-dashed px-6 py-12 text-center transition-colors {dragging
					? 'border-accent bg-accent-soft/40'
					: 'border-line bg-surface hover:border-accent/50'}"
				ondragover={(e) => {
					e.preventDefault();
					dragging = true;
				}}
				ondragleave={() => (dragging = false)}
				ondrop={onDrop}
				onkeydown={(e) => {
					if (e.key === 'Enter' || e.key === ' ') document.getElementById('file-input')?.click();
				}}
				onclick={() => document.getElementById('file-input')?.click()}
			>
				<div class="mb-3 text-3xl" aria-hidden="true">🎙️</div>
				{#if file}
					<p class="font-medium">{fileLabel}</p>
					<p class="mt-1 text-xs text-muted">Klik untuk ganti file</p>
				{:else}
					<p class="font-medium">Tarik &amp; lepas file audio di sini</p>
					<p class="mt-1 text-sm text-muted">
						atau klik untuk memilih — {ALLOWED_EXT.join(', ')} · maks 200 MB
					</p>
				{/if}
				<input id="file-input" type="file" class="sr-only" accept={ALLOWED_EXT.map((e) => '.' + e).join(',')} onchange={onFileInput} />
			</div>

			{#if error}
				<p role="alert" class="rounded-lg bg-danger/10 px-3 py-2 text-sm text-danger">{error}</p>
			{/if}

			<!-- Opsi -->
			<div class="space-y-4 rounded-xl border border-line bg-surface p-5">
				<div>
					<label for="title" class="mb-1 block text-sm font-medium">Judul</label>
					<input id="title" type="text" bind:value={title} placeholder="Otomatis dari nama file"
						class="w-full rounded-lg border border-line bg-bg px-3 py-2 text-sm outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30" />
				</div>
				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<label for="language" class="mb-1 block text-sm font-medium">Bahasa</label>
						<select id="language" bind:value={language}
							class="w-full rounded-lg border border-line bg-bg px-3 py-2 text-sm outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30">
							<option value="">Deteksi otomatis</option>
							<option value="id">Indonesia</option>
							<option value="en">Inggris</option>
							<option value="jv">Jawa</option>
							<option value="su">Sunda</option>
							<option value="ar">Arab</option>
							<option value="ms">Melayu</option>
							<option value="zh">Mandarin</option>
							<option value="ja">Jepang</option>
							<option value="ko">Korea</option>
						</select>
					</div>
					<div>
						<label for="model" class="mb-1 block text-sm font-medium">Model</label>
						<select id="model" bind:value={model}
							class="w-full rounded-lg border border-line bg-bg px-3 py-2 text-sm outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30">
							<option value="whisper-large-v3-turbo">whisper-large-v3-turbo (cepat)</option>
							<option value="whisper-large-v3">whisper-large-v3 (akurat)</option>
						</select>
					</div>
				</div>
				<div>
					<label for="prompt" class="mb-1 block text-sm font-medium">Petunjuk AI <span class="font-normal text-muted">(opsional)</span></label>
					<textarea id="prompt" rows="2" bind:value={prompt} maxlength="500"
						placeholder="Contoh: nama dosen Dr. Rani, istilah: RAG, LLM, fine-tuning"
						class="w-full rounded-lg border border-line bg-bg px-3 py-2 text-sm outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30"></textarea>
				</div>
			</div>

			<!-- Progress / submit -->
			{#if uploading}
				<div aria-live="polite">
					<div class="mb-1 flex justify-between text-sm">
						<span>Mengunggah…</span>
						<span class="font-mono">{Math.round(progress * 100)}%</span>
					</div>
					<div class="h-2.5 w-full overflow-hidden rounded-full bg-surface-2">
						<div class="h-full rounded-full bg-accent transition-[width] duration-200" style="width: {progress * 100}%"></div>
					</div>
				</div>
			{/if}

			<div class="flex justify-end gap-3">
				<a href="/dashboard" class="rounded-lg border border-line bg-surface px-5 py-2.5 font-medium transition-colors hover:bg-surface-2">Batal</a>
				<button type="submit" disabled={!file || uploading}
					class="rounded-lg bg-accent px-5 py-2.5 font-medium text-white transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50">
					{uploading ? 'Mengunggah…' : '▶ Mulai Transkripsi'}
				</button>
			</div>
		</form>
	{/if}
</div>
