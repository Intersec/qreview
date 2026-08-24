<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import ChangeBar from './components/ChangeBar.vue';
import DiffView from './components/DiffView.vue';
import PreferencesDialog from './components/PreferencesDialog.vue';
import ShortcutHelp from './components/ShortcutHelp.vue';
import MergeBar from './components/MergeBar.vue';
import PatchSetBar from './components/PatchSetBar.vue';
import SidePane from './components/SidePane.vue';
import { useReview } from './stores/review';

const review = useReview();
const {
  series,
  changeKey,
  files,
  filePath,
  diff,
  error,
  busy,
  version,
  onMerge,
  mergeBase,
  mergeList,
  split,
  wrap,
  config,
  patchSets,
  patchSet,
  against,
  gerrit,
} = storeToRefs(review);

const comments = computed(() => review.comments());
/// The two settings the browser owns rather than the server.
const codeStyle = computed(() => ({
  '--code-size': `${config.value?.diff.fontSize ?? 12}px`,
  '--tab-width': String(config.value?.diff.tabWidth ?? 4),
}));

const change = computed(() => series.value?.changes.find((c) => c.key === changeKey.value) ?? null);

/// Move to the file before or after the one being read.
function stepFile(by: number) {
  const paths = files.value.filter((f) => !f.binary).map((f) => f.path);
  const at = filePath.value === null ? -1 : paths.indexOf(filePath.value);
  const next = paths[Math.min(Math.max(at + by, 0), paths.length - 1)];
  if (next) {
    review.openFile(next);
  }
}
const lost = computed(() => review.lost());
const copied = ref(false);
const prefs = ref(false);
const helping = ref(false);
const diffView = ref<InstanceType<typeof DiffView> | null>(null);
const side = ref(localStorage.getItem('qreview.side') !== 'hidden');
const pane = ref<InstanceType<typeof SidePane> | null>(null);

function toggleSide() {
  side.value = !side.value;
  localStorage.setItem('qreview.side', side.value ? 'shown' : 'hidden');
}

async function copy(scope: 'change' | 'series') {
  await review.copyExport(scope);
  copied.value = true;
  window.setTimeout(() => {
    copied.value = false;
  }, 2000);
}

/// Move through the review without the mouse.
///
/// j and k walk the files, n and p walk the changes, u swaps the two diff
/// views, [ hides the sidebar, and / jumps to the filter. A key typed into a
/// field is the text of that field and nothing else.
function onKey(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  const typing =
    target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable;
  if (typing || event.metaKey || event.ctrlKey || event.altKey) {
    return;
  }

  switch (event.key) {
    // The same keys Gerrit uses.
    case 'j':
      diffView.value?.moveLine(1);
      break;
    case 'k':
      diffView.value?.moveLine(-1);
      break;
    case 'n':
      diffView.value?.moveHunk(1);
      break;
    case 'p':
      diffView.value?.moveHunk(-1);
      break;
    case ']':
      stepFile(1);
      break;
    case '[':
      stepFile(-1);
      break;
    case 'J':
    case 'K': {
      const keys = series.value?.changes.map((c) => c.key) ?? [];
      const at = changeKey.value === null ? -1 : keys.indexOf(changeKey.value);
      const next = keys[Math.min(Math.max(at + (event.key === 'J' ? 1 : -1), 0), keys.length - 1)];
      if (next) {
        review.openChange(next);
      }
      break;
    }
    case 'c':
      diffView.value?.commentHere();
      break;
    case 'u':
      toggleSide();
      break;
    case ',':
      prefs.value = true;
      break;
    case '?':
      helping.value = true;
      break;
    case 'Escape':
      prefs.value = false;
      helping.value = false;
      break;
    case '/':
      event.preventDefault();
      if (!side.value) {
        toggleSide();
      }
      pane.value?.focusFilter();
      break;
    default:
      return;
  }
}

/// The reader can follow the system or say outright. `system` leaves the
/// attribute off, so the media query decides.
watch(
  () => config.value?.ui.theme,
  (theme) => {
    const root = document.documentElement;
    if (!theme || theme === 'system') {
      delete root.dataset.theme;
    } else {
      root.dataset.theme = theme;
    }
  },
  { immediate: true },
);

onMounted(() => {
  review.load();
  window.addEventListener('keydown', onKey);
});

onBeforeUnmount(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <div class="shell" :style="codeStyle">
    <header class="top-bar">
      <button
        type="button"
        class="side-toggle"
        :aria-pressed="side"
        title="Show or hide the series ( [ )"
        @click="toggleSide"
      >
        {{ side ? '«' : '»' }}
      </button>
      <h1>qreview</h1>
      <p v-if="series" class="repo">{{ series.repo.remote ?? series.repo.root }}</p>

      <span class="bar-actions">
        <button type="button" class="chip" title="This change, as Markdown" @click="copy('change')">
          Copy this change
        </button>
        <button type="button" class="chip" title="The whole series" @click="copy('series')">
          Copy the series
        </button>
        <button type="button" class="chip" title="Keyboard shortcuts ( ? )" @click="helping = true">
          ?
        </button>
        <button type="button" class="chip" title="Preferences ( , )" @click="prefs = true">
          ⚙
        </button>
        <span v-if="copied" role="status" class="copied">copied</span>
        <span class="quiet">{{ version }}</span>
      </span>
    </header>

    <p v-if="error" role="alert" class="error">{{ error }}</p>

    <main v-if="series" class="body" :class="side ? '' : 'no-side'">
      <SidePane
        v-if="side"
        ref="pane"
        :series="series"
        :selected="changeKey"
        :files="files"
        :file-path="filePath"
        :busy="busy"
        @open-change="review.openChange"
        @open-file="review.openFile"
        @mark="review.markChange"
        @more="review.loadMore(5)"
        @review-merge="review.openMerge()"
      />

      <section class="work">
        <ChangeBar
          :change="change"
          :files="files"
          :file-path="filePath"
          :gerrit="gerrit"
          @step="stepFile"
        />
        <PatchSetBar
          :sets="patchSets"
          :current="patchSet"
          :against="against"
          :gerrit="gerrit"
          @open="(ps, base) => review.openPatchSet(ps, base)"
          @fetch="review.fetchPatchSet"
        />
        <MergeBar
          v-if="onMerge"
          :base="mergeBase"
          :list="mergeList"
          @pick="review.openMerge($event)"
          @show-list="review.loadMergeList()"
        />
        <DiffView
          v-if="diff"
          ref="diffView"
          class="grow"
          :diff="diff"
          :split="split"
          :wrap="wrap"
          :comments="comments"
          :lost="lost"
          :placement="review.placement"
          :load-lines="review.loadLines"
          @update:split="review.setSplit"
          @add="review.addComment"
          @edit="(id, body) => review.editComment(id, { body })"
          @remove="review.deleteComment"
        />
        <p v-else class="note">
          {{
            files.length === 0 ? 'This change touches no file.' : 'Pick a file to read its diff.'
          }}
        </p>
      </section>
    </main>

    <p v-else class="note">Reading the repository…</p>

    <PreferencesDialog
      v-if="prefs && config"
      :config="config"
      @save="
        (patch) => {
          review.savePrefs(patch);
          prefs = false;
        }
      "
      @close="prefs = false"
    />

    <ShortcutHelp v-if="helping" @close="helping = false" />
  </div>
</template>
