<script lang="ts">
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<div class="mx-auto flex min-h-screen max-w-5xl flex-col px-6">
	<header class="flex items-center justify-between py-5">
		<a href="/" class="flex items-center gap-2 font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span>
			TranscribeAI
		</a>
		<nav class="flex items-center gap-4 text-sm text-muted">
			<a
				href="https://github.com/Abdurrochman25/transcribe-ai"
				class="transition-colors hover:text-text"
			>
				GitHub
			</a>
			<a href="/login" class="transition-colors hover:text-text">Masuk</a>
			<a
				href="/register"
				class="rounded-full bg-accent px-3 py-1 font-medium text-white transition-colors hover:bg-accent-hover"
			>
				Daftar
			</a>
		</nav>
	</header>

	<main class="flex flex-1 flex-col items-center justify-center gap-10 py-16 text-center">
		<!-- Status wiring API ↔ web -->
		{#if data.api.online}
			<span
				class="inline-flex items-center gap-2 rounded-full border border-success/30 bg-success/10 px-4 py-1.5 text-sm text-success"
			>
				<span class="size-2 rounded-full bg-success" aria-hidden="true"></span>
				API online
				{#if data.api.version}(v{data.api.version}){/if}
			</span>
		{:else}
			<span
				class="inline-flex items-center gap-2 rounded-full border border-danger/30 bg-danger/10 px-4 py-1.5 text-sm text-danger"
			>
				<span class="size-2 rounded-full bg-danger" aria-hidden="true"></span>
				API offline — jalankan backend:
				<code class="font-mono text-xs">cargo run</code>
				di <code class="font-mono text-xs">apps/api</code>
			</span>
		{/if}

		<h1 class="max-w-2xl text-4xl font-bold tracking-tight text-balance sm:text-5xl">
			Ubah audio menjadi teks, dalam hitungan detik.
		</h1>
		<p class="max-w-xl text-lg text-muted text-pretty">
			Upload rekaman kuliah, interview, atau meeting — dapatkan transkrip bertimestamp yang bisa
			diedit dan diekspor ke TXT, SRT, VTT, atau JSON.
		</p>

		<div class="flex flex-wrap items-center justify-center gap-3">
			<a
				href="/register"
				class="rounded-lg bg-accent px-6 py-3 font-medium text-white transition-colors hover:bg-accent-hover"
			>
				Mulai gratis
			</a>
			<a
				href="/login"
				class="rounded-lg border border-line bg-surface px-6 py-3 font-medium transition-colors hover:bg-surface-2"
			>
				Masuk
			</a>
		</div>

		<dl class="grid w-full gap-4 text-left sm:grid-cols-3">
			<div class="rounded-xl border border-line bg-surface p-5 shadow-sm">
				<dt class="font-semibold">⚡ Cepat</dt>
				<dd class="mt-1 text-sm text-muted">
					Inference Groq LPU — audio 10 menit selesai di bawah 1 menit.
				</dd>
			</div>
			<div class="rounded-xl border border-line bg-surface p-5 shadow-sm">
				<dt class="font-semibold">🎯 Akurat</dt>
				<dd class="mt-1 text-sm text-muted">
					Model Whisper large-v3 dengan dukungan 99 bahasa & auto-detect.
				</dd>
			</div>
			<div class="rounded-xl border border-line bg-surface p-5 shadow-sm">
				<dt class="font-semibold">✏️ Bisa diedit</dt>
				<dd class="mt-1 text-sm text-muted">
					Segment bertimestamp, klik untuk lompat audio, ekspor multi-format.
				</dd>
			</div>
		</dl>
	</main>

	<footer class="border-t border-line py-6 text-center text-sm text-muted">
		<div class="flex flex-wrap items-center justify-center gap-x-4 gap-y-1">
			<a href="/privacy" class="transition-colors hover:text-text">Kebijakan Privasi</a>
			<span aria-hidden="true">·</span>
			<a href="/tos" class="transition-colors hover:text-text">Syarat Layanan</a>
			<span aria-hidden="true">·</span>
			<a
				href="https://github.com/Abdurrochman25/transcribe-ai"
				class="transition-colors hover:text-text"
			>
				GitHub
			</a>
		</div>
		<p class="mt-2">Rust (Axum) + SvelteKit + Groq Whisper · MIT License</p>
	</footer>
</div>
