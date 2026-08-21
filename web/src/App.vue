<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import ChangeBar from './components/ChangeBar.vue';
import DiffView from './components/DiffView.vue';
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
  ignoreWs,
  patchSets,
  patchSet,
  against,
  gerrit,
} = storeToRefs(review);

const comments = computed(() => review.comments());
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

  const step = (list: string[], current: string | null, by: number): string | null => {
    if (list.length === 0) {
      return null;
    }
    const at = current === null ? -1 : list.indexOf(current);
    const next = Math.min(Math.max(at + by, 0), list.length - 1);
    return list[next] ?? null;
  };

  const keys = series.value?.changes.map((c) => c.key) ?? [];

  switch (event.key) {
    case 'j':
    case 'k':
      stepFile(event.key === 'j' ? 1 : -1);
      break;
    case 'n':
    case 'p': {
      const key = step(keys, changeKey.value, event.key === 'n' ? 1 : -1);
      if (key) {
        review.openChange(key);
      }
      break;
    }
    case 'u':
      review.setSplit(!split.value);
      break;
    case '[':
      toggleSide();
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

onMounted(() => {
  review.load();
  window.addEventListener('keydown', onKey);
});

onBeforeUnmount(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <div class="shell">
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
        <ChangeBar :change="change" :files="files" :file-path="filePath" @step="stepFile" />
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
          class="grow"
          :diff="diff"
          :split="split"
          :ignore-ws="ignoreWs"
          :comments="comments"
          :lost="lost"
          :placement="review.placement"
          :load-lines="review.loadLines"
          @update:split="review.setSplit"
          @update:ignore-ws="review.setIgnoreWs"
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
  </div>
</template>
