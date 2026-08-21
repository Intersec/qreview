<script setup lang="ts">
import { reactive, watch } from 'vue';
import type { Config } from '@/api/types';

const props = defineProps<{ config: Config }>();
const emit = defineEmits<{ save: [patch: object]; close: [] }>();

/// A copy, so Cancel really cancels.
const draft = reactive({
  context: props.config.diff.context,
  wrap: props.config.diff.wrap,
  ignoreWhitespace: props.config.diff.ignoreWhitespace,
  tabWidth: props.config.diff.tabWidth,
  fontSize: props.config.diff.fontSize,
  syntax: props.config.diff.syntax,
  view: props.config.ui.diff,
});

watch(
  () => props.config,
  (fresh) => {
    Object.assign(draft, fresh.diff, { view: fresh.ui.diff });
  },
);

const CONTEXTS = [3, 10, 25, 50];

function save() {
  emit('save', {
    diff: {
      context: Number(draft.context),
      wrap: draft.wrap,
      ignoreWhitespace: draft.ignoreWhitespace,
      tabWidth: Number(draft.tabWidth),
      fontSize: Number(draft.fontSize),
      syntax: draft.syntax,
    },
    ui: { diff: draft.view },
  });
}
</script>

<template>
  <div class="veil" @click.self="emit('close')">
    <form
      class="panel"
      role="dialog"
      aria-modal="true"
      aria-label="Preferences"
      @submit.prevent="save"
    >
      <h2 class="panel-title">Preferences</h2>

      <label class="row">
        <span>Context</span>
        <select v-model="draft.context" class="picker">
          <option v-for="lines in CONTEXTS" :key="lines" :value="lines">{{ lines }} lines</option>
          <option :value="2000">the whole file</option>
        </select>
      </label>

      <label class="row">
        <span>Diff view</span>
        <select v-model="draft.view" class="picker">
          <option value="unified">unified</option>
          <option value="side-by-side">side by side</option>
        </select>
      </label>

      <label class="row">
        <span>Wrap a long line</span>
        <input v-model="draft.wrap" type="checkbox" />
      </label>

      <label class="row">
        <span>Ignore whitespace</span>
        <input v-model="draft.ignoreWhitespace" type="checkbox" />
      </label>

      <label class="row">
        <span>Syntax colours</span>
        <input v-model="draft.syntax" type="checkbox" />
      </label>

      <label class="row">
        <span>Tab width</span>
        <input v-model="draft.tabWidth" type="number" min="1" max="16" class="picker number" />
      </label>

      <label class="row">
        <span>Font size</span>
        <input v-model="draft.fontSize" type="number" min="8" max="24" class="picker number" />
      </label>

      <p class="panel-note">
        Kept in <code>~/.config/qreview/config.json</code>, so the next run starts this way.
      </p>

      <div class="panel-foot">
        <button type="button" class="action" @click="emit('close')">Cancel</button>
        <button type="submit" class="action">Save</button>
      </div>
    </form>
  </div>
</template>
