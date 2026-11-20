<script setup lang="ts">
import { computed, reactive, watch } from 'vue';
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
  theme: props.config.ui.theme,
});

/// The fields that the reader touched. Only those are sent: a value shown
/// may come from the .qreview.json of the repository, or be a theme that
/// did not load, and it must never replace what the user wrote in the
/// file. A touched field is sent even when it equals the value shown, so
/// the reader can clear a theme that did not load.
const touched = reactive(new Set<string>());

function touch(event: Event) {
  const name = (event.target as HTMLInputElement).name;
  if (name) {
    touched.add(name);
  }
}

watch(
  () => props.config,
  (fresh) => {
    Object.assign(draft, fresh.diff, { view: fresh.ui.diff, theme: fresh.ui.theme });
    touched.clear();
  },
);

const CONTEXTS = [3, 10, 25, 50];

/// The names of the themes, bundled or of the user. A file that did not
/// load is not offered: the terminal says why.
function themeNames(bundled: boolean): string[] {
  const themes = Object.entries(props.config.ui.themes ?? {});
  return themes
    .filter(([, theme]) => theme.bundled === bundled)
    .map(([name]) => name)
    .sort();
}
const userThemes = computed(() => themeNames(false));
const bundledThemes = computed(() => themeNames(true));

/// The fields of the diff section, with the ones that the panel reads as text.
const DIFF = ['context', 'wrap', 'ignoreWhitespace', 'tabWidth', 'fontSize', 'syntax'] as const;
const NUMBERS: readonly string[] = ['context', 'tabWidth', 'fontSize'];

function save() {
  const diff: Record<string, unknown> = {};
  for (const name of DIFF.filter((n) => touched.has(n))) {
    diff[name] = NUMBERS.includes(name) ? Number(draft[name]) : draft[name];
  }
  const ui: Record<string, string> = {};
  if (touched.has('view')) {
    ui.diff = draft.view;
  }
  if (touched.has('theme')) {
    ui.theme = draft.theme;
  }
  // An empty section stays out, so the file of the user gets no empty block.
  emit('save', {
    ...(Object.keys(diff).length ? { diff } : {}),
    ...(Object.keys(ui).length ? { ui } : {}),
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
      @input="touch"
      @change="touch"
    >
      <h2 class="panel-title">Preferences</h2>

      <label class="row">
        <span>Context</span>
        <select v-model="draft.context" name="context" class="picker">
          <option v-for="lines in CONTEXTS" :key="lines" :value="lines">{{ lines }} lines</option>
          <option :value="2000">the whole file</option>
        </select>
      </label>

      <label class="row">
        <span>Diff view</span>
        <select v-model="draft.view" name="view" class="picker">
          <option value="unified">unified</option>
          <option value="side-by-side">side by side</option>
        </select>
      </label>

      <label class="row">
        <span>Theme</span>
        <select v-model="draft.theme" name="theme" class="picker">
          <option value="system">follow the system</option>
          <option value="light">light</option>
          <option value="dark">dark</option>
          <optgroup v-if="userThemes.length" label="Your themes">
            <option v-for="name in userThemes" :key="name" :value="name">{{ name }}</option>
          </optgroup>
          <optgroup v-if="bundledThemes.length" label="Bundled themes">
            <option v-for="name in bundledThemes" :key="name" :value="name">{{ name }}</option>
          </optgroup>
        </select>
      </label>

      <label class="row">
        <span>Wrap a long line</span>
        <input v-model="draft.wrap" name="wrap" type="checkbox" />
      </label>

      <label class="row">
        <span>Ignore whitespace</span>
        <input v-model="draft.ignoreWhitespace" name="ignoreWhitespace" type="checkbox" />
      </label>

      <label class="row">
        <span>Syntax colours</span>
        <input v-model="draft.syntax" name="syntax" type="checkbox" />
      </label>

      <label class="row">
        <span>Tab width</span>
        <input
          v-model="draft.tabWidth"
          name="tabWidth"
          type="number"
          min="1"
          max="16"
          class="picker number"
        />
      </label>

      <label class="row">
        <span>Font size</span>
        <input
          v-model="draft.fontSize"
          name="fontSize"
          type="number"
          min="8"
          max="24"
          class="picker number"
        />
      </label>

      <p class="panel-note">
        Kept in <code>~/.config/qreview/config.json</code>, so the next run starts this way. Your
        themes are the <code>.css</code> files in <code>~/.config/qreview/themes/</code>. A file
        there replaces the bundled theme of the same name.
      </p>

      <div class="panel-foot">
        <button type="button" class="action" @click="emit('close')">Cancel</button>
        <button type="submit" class="action">Save</button>
      </div>
    </form>
  </div>
</template>
