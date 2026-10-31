<script lang="ts">
	import { auth } from '../../lib/stores/auth.svelte';
import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	// Guard: tunggu pemeriksaan sesi, redirect bila tidak login.
	let checked = $state(false);

	$effect(() => {
		auth.init().then(() => {
			checked = true;
		});
	});

	$effect(() => {
		if (checked && !auth.user) {
			const next = page.url.pathname;
			window.location.href = `/login?next=${encodeURIComponent(next)}`;
		}
	});
</script>

{#if auth.user}
	{@render children()}
{:else if !checked}
	<div class="flex min-h-screen items-center justify-center" aria-live="polite">
		<div class="flex items-center gap-3 text-muted">
			<span
				class="size-5 animate-spin rounded-full border-2 border-line border-t-accent"
				aria-hidden="true"></span>
			Memeriksa sesi…
		</div>
	</div>
{/if}
