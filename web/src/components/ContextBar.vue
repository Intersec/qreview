<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{
  from: number;
  to: number;
  columns: number;
  busy: boolean;
  /// True when the hunk sits above this bar, so a short step opens the top
  /// of the gap rather than the bottom.
  hunkAbove?: boolean;
}>();
const emit = defineEmits<{ open: [from: number, to: number] }>();

/// How many lines one click opens when the gap is long.
const STEP = 10;

const count = computed(() => props.to - props.from + 1);
</script>

<template>
  <tr class="context-bar">
    <td :colspan="columns">
      <button type="button" class="context-button" :disabled="busy" @click="emit('open', from, to)">
        +{{ count }} common line{{ count === 1 ? '' : 's' }}
      </button>
      <button
        v-if="count > STEP"
        type="button"
        class="context-button"
        :disabled="busy"
        @click="hunkAbove ? emit('open', from, from + STEP - 1) : emit('open', to - STEP + 1, to)"
      >
        +{{ STEP }}
      </button>
    </td>
  </tr>
</template>
