<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{ from: number; to: number; columns: number; busy: boolean }>();
const emit = defineEmits<{ open: [from: number, to: number] }>();

/// How many lines one short step opens.
const STEP = 10;

const count = computed(() => props.to - props.from + 1);
const stepping = computed(() => count.value > STEP);
</script>

<template>
  <tr class="context-bar">
    <td :colspan="columns">
      <span class="context-group">
        <button
          type="button"
          class="context-button"
          :disabled="busy"
          @click="emit('open', from, to)"
        >
          +{{ count }} common line{{ count === 1 ? '' : 's' }}
        </button>

        <!-- Stacked the way Gerrit stacks them: the upper one opens the
             lines under the code above, the lower one the lines over the
             code below. -->
        <span v-if="stepping" class="context-steps">
          <button
            type="button"
            class="context-button"
            title="Open the ten lines under the code above"
            :disabled="busy"
            @click="emit('open', from, from + STEP - 1)"
          >
            +{{ STEP }} ↑
          </button>
          <button
            type="button"
            class="context-button"
            title="Open the ten lines over the code below"
            :disabled="busy"
            @click="emit('open', to - STEP + 1, to)"
          >
            +{{ STEP }} ↓
          </button>
        </span>
      </span>
    </td>
  </tr>
</template>
