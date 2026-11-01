<script setup lang="ts">
import { computed } from 'vue';
import { head, type BlameLine } from '@/diff/blame';

const props = defineProps<{
  at: BlameLine;
  /// The cell under the pointer, in the coordinates of the window.
  cell: DOMRect;
  /// What a click on the line does, or null when it does nothing.
  action: string | null;
}>();

/// Beside the cell, so the name stays in sight. Under its top while the
/// window has room below, over its bottom otherwise.
const style = computed(() => {
  const left = `${props.cell.right + 8}px`;
  if (props.cell.top < window.innerHeight / 2) {
    return { left, top: `${props.cell.top}px` };
  }
  return { left, bottom: `${window.innerHeight - props.cell.bottom}px` };
});
</script>

<template>
  <div class="blame-tip" role="tooltip" :style="style">
    <p v-for="line in head(at.commit)" :key="line">{{ line }}</p>
    <p class="blame-tip-message">{{ at.commit.message.trimEnd() }}</p>
    <p v-if="action" class="blame-tip-action">{{ action }}</p>
  </div>
</template>
