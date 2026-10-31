<script lang="ts">
	import { auth } from '../../../lib/stores/auth.svelte';

	const user = $derived(auth.user);
</script>

<div class="mx-auto max-w-5xl px-6">
	<header class="flex items-center justify-between py-5">
		<a href="/dashboard" class="flex items-center gap-2 font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span>
			TranscribeAI
		</a>
		<div class="flex items-center gap-4">
			<span class="hidden text-sm text-muted sm:inline">{user?.email}</span>
			<button
				type="button"
				onclick={() => auth.logout()}
				class="rounded-lg border border-line bg-surface px-4 py-2 text-sm font-medium transition-colors hover:bg-surface-2"
			>
				Keluar
			</button>
		</div>
	</header>

	<main class="py-8">
		<div class="mb-8">
			<h1 class="text-2xl font-bold tracking-tight">Halo, {user?.name} 👋</h1>
			<p class="mt-1 text-muted">Siap mentranskribasi audio pertamamu?</p>
		</div>

		<!-- TODO(fase 2): ganti dengan komponen FileDropzone + riwayat asli -->
		<div class="rounded-xl border border-dashed border-line bg-surface p-12 text-center">
			<div class="mx-auto mb-4 flex size-12 items-center justify-center rounded-full bg-accent-soft text-2xl" aria-hidden="true">
				🎙️
			</div>
			<h2 class="font-semibold">Belum ada transkripsi</h2>
			<p class="mx-auto mt-1 max-w-md text-sm text-muted">
				Fitur upload & transkripsi dibangun di Fase 2–3. Saat ini sesi login-mu sudah aktif
				penuh — dashboard ini akan menampung riwayat transkripsi.
			</p>
			<span class="mt-4 inline-block rounded-full bg-surface-2 px-3 py-1 font-mono text-xs text-muted">
				plan: {user?.plan ?? 'free'}
			</span>
		</div>
	</main>
</div>
