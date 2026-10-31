<script lang="ts">
	import '../app.css';
	import favicon from '#lib/assets/favicon.svg';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>TranscribeAI — Ubah audio jadi teks</title>
	<meta
		name="description"
		content="TranscribeAI: transkripsi audio berbasis AI yang cepat, akurat, dan mudah diedit."
	/>
</svelte:head>

{@render children()}
