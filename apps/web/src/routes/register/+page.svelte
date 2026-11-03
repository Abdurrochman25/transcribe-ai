<script lang="ts">
	import { goto } from '$app/navigation';
	import { auth } from '../../lib/stores/auth.svelte';
	import { ApiError } from '../../lib/api/types';

	let name = $state('');
	let email = $state('');
	let password = $state('');
	let inviteCode = $state('');
	let error = $state<string | null>(null);
	let fieldErrors = $state<Record<string, string>>({});
	let submitting = $state(false);

	// Sudah login → langsung ke dashboard.
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
			await auth.register(name.trim(), email.trim(), password, inviteCode);
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

<div class="mx-auto flex min-h-screen max-w-sm flex-col justify-center px-6 py-12">
	<div class="mb-8 text-center">
		<a href="/" class="inline-flex items-center gap-2.5 text-[15px] font-semibold tracking-tight">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
				<rect x="1" y="5" width="2" height="6" rx="1" fill="currentColor" />
				<rect x="4.25" y="3" width="2" height="10" rx="1" fill="currentColor" />
				<rect x="7.5" y="1" width="2" height="14" rx="1" fill="currentColor" />
				<rect x="10.75" y="4" width="2" height="8" rx="1" fill="currentColor" />
				<rect x="14" y="6" width="2" height="4" rx="1" fill="currentColor" />
			</svg>
			TranscribeAI
		</a>
		<h1 class="mt-5 text-xl font-semibold tracking-tight">Buat akun baru</h1>
		<p class="mt-1 text-sm text-muted">Gratis, tanpa kartu kredit.</p>
	</div>

	<form onsubmit={submit} class="space-y-4 rounded-(--radius-card) border border-line bg-surface p-6" novalidate>
		{#if error}
			<p role="alert" class="rounded-(--radius-control) bg-pill-negative-bg px-3 py-2 text-sm text-pill-negative-ink">
				{error}
			</p>
		{/if}

		<div>
			<label for="name" class="mb-1.5 block text-sm font-medium">Nama</label>
			<input
				id="name"
				type="text"
				autocomplete="name"
				bind:value={name}
				required
				class="h-10 w-full rounded-(--radius-control) border border-control bg-surface px-3 text-sm outline-hidden placeholder:text-faint focus:border-muted"
				placeholder="Nama kamu"
			/>
			{#if fieldErrors.name}<p class="mt-1 text-xs text-danger">{fieldErrors.name}</p>{/if}
		</div>

		<div>
			<label for="email" class="mb-1.5 block text-sm font-medium">Email</label>
			<input
				id="email"
				type="email"
				autocomplete="email"
				bind:value={email}
				required
				class="h-10 w-full rounded-(--radius-control) border border-control bg-surface px-3 text-sm outline-hidden placeholder:text-faint focus:border-muted"
				placeholder="nama@email.com"
			/>
			{#if fieldErrors.email}<p class="mt-1 text-xs text-danger">{fieldErrors.email}</p>{/if}
		</div>

		<div>
			<div class="mb-1.5 flex items-baseline justify-between">
				<label for="password" class="text-sm font-medium">Password</label>
				<span class="text-xs text-muted">Min. 10 karakter, huruf &amp; angka</span>
			</div>
			<input
				id="password"
				type="password"
				autocomplete="new-password"
				bind:value={password}
				required
				minlength="10"
				class="h-10 w-full rounded-(--radius-control) border border-control bg-surface px-3 text-sm outline-hidden focus:border-muted"
			/>
			{#if fieldErrors.password}<p class="mt-1 text-xs text-danger">{fieldErrors.password}</p>{/if}
		</div>

		<div>
			<label for="invite_code" class="mb-1.5 block text-sm font-medium">
				Kode undangan <span class="font-normal text-muted">(opsional)</span>
			</label>
			<input
				id="invite_code"
				type="text"
				bind:value={inviteCode}
				autocomplete="off"
				class="h-10 w-full rounded-(--radius-control) border border-control bg-surface px-3 text-sm outline-hidden placeholder:text-faint focus:border-muted"
				placeholder="Kosongkan bila tidak punya"
			/>
			{#if fieldErrors.invite_code}<p class="mt-1 text-xs text-danger">{fieldErrors.invite_code}</p>{/if}
		</div>

		<button
			type="submit"
			disabled={submitting}
			class="h-10 w-full rounded-(--radius-control) bg-accent text-sm font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
		>
			{submitting ? 'Membuat akun…' : 'Daftar'}
		</button>
	</form>

	<p class="mt-6 text-center text-sm text-muted">
		Sudah punya akun?
		<a href="/login" class="font-medium text-text hover:underline">Masuk</a>
	</p>
</div>
