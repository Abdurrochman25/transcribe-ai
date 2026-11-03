<script lang="ts">
	import { page } from '$app/state';
	import { auth } from '../../lib/stores/auth.svelte';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	/** DEV-only: ?preview melewati guard agar layar auth bisa diinspeksi (taste-skill). */
	const preview = $derived(import.meta.env.DEV && page.url.searchParams.has('preview'));

	let checked = $state(false);

	$effect(() => {
		if (preview) {
			checked = true;
			return;
		}
		auth.init().then(() => {
			checked = true;
		});
	});

	$effect(() => {
		if (checked && !auth.user && !preview) {
			const next = page.url.pathname;
			window.location.href = `/login?next=${encodeURIComponent(next)}`;
		}
	});

	const navItems = [
		{ href: '/dashboard', label: 'Riwayat', active: (p: string) => p === '/dashboard' || p.startsWith('/t/') },
		{ href: '/upload', label: 'Upload', active: (p: string) => p.startsWith('/upload') }
	];

	const withParam = (href: string) => (preview ? `${href}?preview=1` : href);
</script>

<!-- Shell "no sidebar" (sheet achromatic-ledger): bar 48px di ground, satu kolom dengan konten. -->
<div class="sticky top-0 z-20 border-b border-line bg-bg">
	<header class="mx-auto flex h-12 max-w-6xl items-center justify-between gap-4 px-6">
		<a
			href={withParam('/dashboard')}
			class="flex items-center gap-2.5 text-[15px] font-semibold tracking-tight"
		>
			<!-- Mark produk: gelombang audio 5 batang (digambar untuk produk ini) -->
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
				<rect x="1" y="5" width="2" height="6" rx="1" fill="currentColor" />
				<rect x="4.25" y="3" width="2" height="10" rx="1" fill="currentColor" />
				<rect x="7.5" y="1" width="2" height="14" rx="1" fill="currentColor" />
				<rect x="10.75" y="4" width="2" height="8" rx="1" fill="currentColor" />
				<rect x="14" y="6" width="2" height="4" rx="1" fill="currentColor" />
			</svg>
			TranscribeAI
		</a>
		<nav class="flex items-center gap-5 text-sm" aria-label="Navigasi utama">
			{#each navItems as item (item.href)}
				<a
					href={withParam(item.href)}
					class={item.active(page.url.pathname)
						? 'font-medium text-text'
						: 'text-muted transition-colors hover:text-text'}
					aria-current={item.active(page.url.pathname) ? 'page' : undefined}
				>
					{item.label}
				</a>
			{/each}
			{#if !preview}
				<span class="hidden text-muted lg:inline">{auth.user?.email}</span>
				<button type="button" onclick={() => auth.logout()} class="text-muted transition-colors hover:text-text">
					Keluar
				</button>
			{/if}
		</nav>
	</header>
</div>

{#if auth.user || preview}
	{@render children()}
{:else if !checked}
	<div class="flex min-h-[80vh] items-center justify-center" aria-live="polite">
		<div class="flex items-center gap-3 text-muted">
			<span
				class="size-5 animate-spin rounded-full border-2 border-line border-t-text"
				aria-hidden="true"></span>
			Memeriksa sesi…
		</div>
	</div>
{/if}
