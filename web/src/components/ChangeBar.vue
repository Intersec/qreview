<script setup lang="ts">
import { computed } from 'vue';
import type { ChangeSummary, FileEntry, GerritChange } from '@/api/types';

const props = defineProps<{
  change: ChangeSummary | null;
  files: FileEntry[];
  filePath: string | null;
  /// The change on the server, when the remote names one.
  gerrit: GerritChange | null;
}>();
const emit = defineEmits<{ step: [by: number] }>();

const readable = computed(() => props.files.filter((f) => !f.binary));
const at = computed(() => readable.value.findIndex((f) => f.path === props.filePath));
</script>

<template>
  <div v-if="change" class="change-bar">
    <span class="subject">{{ change.subject }}</span>
    <code class="quiet">{{ change.commit.slice(0, 12) }}</code>
    <a
      v-if="change.changeId && gerrit"
      class="change-id"
      :href="gerrit.url"
      target="_blank"
      rel="noreferrer"
      :title="`Change ${gerrit.number} on Gerrit`"
      >{{ change.changeId }}</a
    >
    <code v-else-if="change.changeId" class="quiet change-id">{{ change.changeId }}</code>
    <span v-else class="tag">no Change-Id</span>

    <span class="bar-actions">
      <span v-if="at >= 0" class="quiet">File {{ at + 1 }} of {{ readable.length }}</span>
      <button
        type="button"
        class="chip"
        :disabled="at <= 0"
        title="The file before ( k )"
        @click="emit('step', -1)"
      >
        Prev
      </button>
      <button
        type="button"
        class="chip"
        :disabled="at < 0 || at >= readable.length - 1"
        title="The next file ( j )"
        @click="emit('step', 1)"
      >
        Next
      </button>
    </span>
  </div>
</template>
