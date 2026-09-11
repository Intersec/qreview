<script setup lang="ts">
import { computed } from 'vue';
import type { Boundary } from '@/api/types';

const props = defineProps<{ boundary: Boundary; batch: number; busy: boolean }>();
const emit = defineEmits<{ more: [] }>();

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

    <!-- What the button would load first. It stands last, under everything
       that says why the walk stopped: a hash alone says nothing, and the
       reader decides whether to go on from what is written there. -->
    <p v-if="boundary.commit" class="quiet mt-1">
      next <code>{{ boundary.commit.slice(0, 12) }}</code> {{ boundary.subject }}
    </p>

    <p class="mt-2 flex flex-wrap gap-2">
      <button
        v-if="boundary.commit"
        type="button"
        class="context-button"
        :disabled="busy"
        @click="emit('more')"
      >
        Load up to {{ batch }} older
      </button>
    </p>
  </section>
</template>
