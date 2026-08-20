<script setup lang="ts">
import { computed } from 'vue';
import type { Boundary } from '@/api/types';

const props = defineProps<{ boundary: Boundary; busy: boolean }>();
const emit = defineEmits<{ more: []; reviewMerge: [] }>();

const title = computed(() => {
  switch (props.boundary.kind) {
    case 'merge':
      return 'Merge';
    case 'tag':
      return 'Tag';
    case 'base':
      return 'Base of the series';
    case 'guess':
      return 'Guessed start';
    case 'batch':
      return 'More to load';
    case 'root':
      return 'Start of the history';
  }
  return 'Boundary';
});
</script>

<template>
  <section class="boundary">
    <h3>{{ title }}</h3>
    <p>{{ boundary.reason }}</p>

    <p v-if="boundary.guessed" class="quiet">This is a guess. Load more to go further back.</p>

    <template v-if="boundary.merge">
      <p class="mt-1 font-medium">{{ boundary.merge.subject }}</p>
      <p v-for="(parent, i) in boundary.merge.parents" :key="parent.commit" class="quiet">
        parent {{ i + 1 }} <code>{{ parent.commit.slice(0, 12) }}</code> {{ parent.name }}
        <span v-if="parent.remote">(a remote branch)</span>
      </p>
    </template>

    <p class="mt-2 flex flex-wrap gap-2">
      <button
        v-if="boundary.merge"
        type="button"
        class="context-button"
        @click="emit('reviewMerge')"
      >
        Review the merge
      </button>
      <button
        v-if="boundary.commit"
        type="button"
        class="context-button"
        :disabled="busy"
        @click="emit('more')"
      >
        Load 5 older
      </button>
    </p>
  </section>
</template>
