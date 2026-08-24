<script setup lang="ts">
// A veil with a spinner, which waits before it shows.
//
// Most reads land in a few tens of milliseconds, and a spinner that flashes
// for those is worse than none. This one appears when the wait is long
// enough for a reader to wonder whether anything is happening.

import { onUnmounted, ref, watch } from 'vue';

const props = withDefaults(defineProps<{ when: boolean; after?: number; label?: string }>(), {
  after: 200,
  label: 'Loading',
});

const shown = ref(false);
let timer: number | undefined;

watch(
  () => props.when,
  (waiting) => {
    window.clearTimeout(timer);
    if (!waiting) {
      shown.value = false;
      return;
    }
    timer = window.setTimeout(() => {
      shown.value = true;
    }, props.after);
  },
  { immediate: true },
);

onUnmounted(() => window.clearTimeout(timer));
</script>

<template>
  <p v-if="shown" class="waiting" role="status">
    <span class="ring" aria-hidden="true"></span>
    {{ label }}…
  </p>
</template>
