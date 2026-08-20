<script setup lang="ts">
import { version } from './version';
</script>

<template>
  <main class="mx-auto max-w-2xl p-8 font-sans">
    <h1 class="text-2xl font-semibold">qreview</h1>
    <p class="mt-2 text-slate-600 dark:text-slate-400">
      The scaffold works. The interface arrives in M2 — see
      <code>roadmap/plan.md</code>.
    </p>
    <p class="mt-4 text-sm text-slate-500 dark:text-slate-400">{{ version }}</p>
  </main>
</template>
