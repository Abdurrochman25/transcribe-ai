<script lang="ts">
	import { goto } from '$app/navigation';
	import { auth } from '../../lib/stores/auth.svelte';
	import { ApiError } from '../../lib/api/types';

	let email = $state('');
	let password = $state('');
	let error = $state<string | null>(null);
	let fieldErrors = $state<Record<string, string>>({});
	let submitting = $state(false);

	// Sudah login (refresh cookie masih valid) → tak perlu form, langsung masuk.
	$effect(() => {
		auth.init().then(() => {
			if (auth.user) goto('/dashboard', { replaceState: true });
		});
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = null;
		fieldErrors = {};
		submitting = true;
		try {
			await auth.login(email.trim(), password);
			await goto('/dashboard');
		} catch (err) {
			if (err instanceof ApiError) {
				error = err.message;
				for (const d of err.details ?? []) fieldErrors[d.field] = d.issue;
			} else {
				error = 'Tidak bisa menghubungi server. Coba lagi.';
			}
		} finally {
			submitting = false;
		}
	}
</script>

<div class="mx-auto flex min-h-screen max-w-md flex-col justify-center px-6 py-12">
	<div class="mb-8 text-center">
		<a href="/" class="text-lg font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span> TranscribeAI
		</a>
		<h1 class="mt-4 text-2xl font-bold tracking-tight">Masuk ke akunmu</h1>
		<p class="mt-1 text-sm text-muted">Transkripsi audio berikutnya menunggu.</p>
	</div>

	<form
		onsubmit={submit}
		class="space-y-4 rounded-xl border border-line bg-surface p-6 shadow-sm"
		novalidate
	>
		{#if error}
			<p role="alert" class="rounded-lg bg-danger/10 px-3 py-2 text-sm text-danger">{error}</p>
		{/if}

		<div>
			<label for="email" class="mb-1 block text-sm font-medium">Email</label>
			<input
				id="email"
				type="email"
				autocomplete="email"
				bind:value={email}
				required
				class="w-full rounded-lg border border-line bg-bg px-3 py-2 text-sm outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30"
				placeholder="nama@email.com"
			/>
			{#if fieldErrors.email}<p class="mt-1 text-xs text-danger">{fieldErrors.email}</p>{/if}
		</div>

		<div>
			<label for="password" class="mb-1 block text-sm font-medium">Password</label>
			<input
				id="password"
				type="password"
				autocomplete="current-password"
				bind:value={password}
				required
				class="w-full rounded-lg border border-line bg-bg px-3 py-2 text-sm outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30"
				placeholder="••••••••••"
			/>
			{#if fieldErrors.password}
				<p class="mt-1 text-xs text-danger">{fieldErrors.password}</p>
			{/if}
		</div>

		<button
			type="submit"
			disabled={submitting}
			class="w-full rounded-lg bg-accent px-4 py-2.5 font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
		>
			{submitting ? 'Memproses…' : 'Masuk'}
		</button>
	</form>

	<p class="mt-6 text-center text-sm text-muted">
		Belum punya akun?
		<a href="/register" class="font-medium text-accent hover:underline">Daftar gratis</a>
	</p>
</div>
