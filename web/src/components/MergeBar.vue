<script setup lang="ts">
import type { MergeListItem } from '@/api/types';

/// `base` is what the base selector holds. Absent is the auto-merge.
defineProps<{ base: string | undefined; list: MergeListItem[] }>();
const emit = defineEmits<{ showList: [] }>();
</script>

<template>
  <div class="merge-bar">
    <div class="flex flex-wrap items-center gap-2">
      <h3>Merge</h3>
      <button type="button" class="context-button ml-auto" @click="emit('showList')">
        What it brings in
      </button>
    </div>

    <p v-if="base === undefined" class="quiet mt-1">
      The auto-merge shows what a person resolved. The rest was already reviewed on the branch it
      came from.
    </p>

    <ol v-if="list.length" class="mt-2 space-y-0.5">
      <li v-for="item in list" :key="item.commit" class="truncate">
        <code>{{ item.commit.slice(0, 12) }}</code>
        <span class="ml-2">{{ item.subject }}</span>
        <span class="quiet ml-2">{{ item.author }}</span>
      </li>
    </ol>
  </div>
</template>
