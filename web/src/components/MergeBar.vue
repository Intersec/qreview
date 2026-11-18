<script setup lang="ts">
import type { MergeListItem } from '@/api/types';

/// `base` is what the base selector holds. Absent is the auto-merge.
defineProps<{ base: string | undefined; list: MergeListItem[] }>();
const emit = defineEmits<{ showList: [] }>();
</script>

<template>
  <div class="merge-bar px-3 py-2 text-xs">
    <div class="flex flex-wrap items-center gap-2">
      <span class="title font-semibold uppercase tracking-wide"> Merge </span>
      <button type="button" class="ml-auto px-2 py-0.5" @click="emit('showList')">
        What it brings in
      </button>
    </div>

    <p v-if="base === undefined" class="note mt-1">
      The auto-merge shows what a person resolved. The rest was already reviewed on the branch it
      came from.
    </p>

    <ol v-if="list.length" class="mt-2 space-y-0.5">
      <li v-for="item in list" :key="item.commit" class="truncate">
        <code>{{ item.commit.slice(0, 12) }}</code>
        <span class="ml-2">{{ item.subject }}</span>
        <span class="author ml-2">{{ item.author }}</span>
      </li>
    </ol>
  </div>
</template>
