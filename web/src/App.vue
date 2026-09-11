<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { format, parse, same, type Place } from '@/place';
import { storeToRefs } from 'pinia';
import ChangeBar from './components/ChangeBar.vue';
import DiffView from './components/DiffView.vue';
import PreferencesDialog from './components/PreferencesDialog.vue';
import ShortcutHelp from './components/ShortcutHelp.vue';
import LoadingVeil from './components/LoadingVeil.vue';
import MergeBar from './components/MergeBar.vue';
import PaneSplit from './components/PaneSplit.vue';
import PatchSetBar from './components/PatchSetBar.vue';
import SidePane from './components/SidePane.vue';
import { useReview } from './stores/review';
import type { Side } from './api/types';

const review = useReview();
const {
  series,
  changeKey,
  files,
  filePath,
  diff,
  error,
  busy,
  loadingFiles,
  loadingDiff,
  written,
  total,
  countOf,
  batchSize,
  inFile,
  release,
  version,
  build,
  posted,
  readingOlder,
  currentSha,
  onMerge,
  mergeList,
  split,
  wrap,
  config,
  patchSets,
  patchSet,
  against,
  gerrit,
  place,
} = storeToRefs(review);

const comments = computed(() => review.comments());
/// How many comments the change on the screen carries.
const here = computed(() => (changeKey.value ? (countOf.value.get(changeKey.value) ?? 0) : 0));
/// The two settings the browser owns rather than the server.
const codeStyle = computed(() => ({
  '--code-size': `${config.value?.diff.fontSize ?? 12}px`,
  '--tab-width': String(config.value?.diff.tabWidth ?? 4),
}));

const change = computed(() => series.value?.changes.find((c) => c.key === changeKey.value) ?? null);

/// A move of the reader, in the browser history.
///
/// The place the reader is going to is written first, so Back answers the
/// moment they click. What the move landed on is written over that entry,
/// because opening a change lands on a file of it, and the reader made one
/// move rather than two.
///
/// Only the newest move writes what it landed on. Picking a file while the
/// change is still loading is two moves that overlap, and the older one
/// must not put its place over the newer one.
let moves = 0;

async function goTo(intent: Place | null, move: Promise<unknown>) {
  const mine = (moves += 1);
  write(intent, 'push');
  await move;

  if (mine === moves) {
    write(place.value, 'replace');
  }
}

function write(at: Place | null, how: 'push' | 'replace') {
  const shown = parse(location.hash);
  if (!at || same(at, shown)) {
    return;
  }

  // The first place of a run takes the address the browser opened, so going
  // back from it leaves the page instead of landing on a bare URL.
  if (how === 'push' && shown !== null) {
    history.pushState(null, '', format(at));
  } else {
    history.replaceState(null, '', format(at));
  }
}

/// The other way round: the browser hands a place back, and the reader is
/// put there. Nothing is written, because that entry is already the one the
/// history is on.
function onPop() {
  const at = parse(location.hash);
  if (at) {
    void review.openPlace(at);
  }
}

function goToChange(key: string) {
  return goTo({ change: key }, review.openChange(key));
}

function goToFile(path: string) {
  const at = place.value;
  return goTo(at && { ...at, file: path }, review.openFile(path));
}

function goToVersion(ps: number | undefined, base: string | undefined) {
  const at = place.value;
  return goTo(at && { ...at, ps, base }, review.openPatchSet(ps, base));
}

/// Open the place a comment speaks of: the change, then the file, then the
/// line the keyboard lands on.
async function goToComment(key: string, file: string, side: Side, line: number | null) {
  const move = async () => {
    if (key !== changeKey.value) {
      await review.openChange(key);
    }
    if (file !== '' && file !== filePath.value) {
      await review.openFile(file);
    }
  };
  await goTo({ change: key, file: file === '' ? undefined : file }, move());

  if (line !== null) {
    await nextTick();
    diffView.value?.revealLine(side, line);
  }
}

/// Move to the file before or after the one being read.
function stepFile(by: number) {
  const paths = files.value.filter((f) => !f.binary).map((f) => f.path);
  const at = filePath.value === null ? -1 : paths.indexOf(filePath.value);
  const next = paths[Math.min(Math.max(at + by, 0), paths.length - 1)];
  if (next) {
    void goToFile(next);
  }
}
const stranded = computed(() => review.stranded());
/// What Gerrit already holds for the change being read, and the part of it
/// this version has no line for.
const postedStranded = computed(() => review.postedStranded());
/// The files of the change, so the diff can tell a remark about a file that
/// is still here from one about a file the change no longer touches.
const paths = computed(() => files.value.map((file) => file.path));
const copied = ref(false);
const prefs = ref(false);
const helping = ref(false);
const diffView = ref<InstanceType<typeof DiffView> | null>(null);
const side = ref(localStorage.getItem('qreview.side') !== 'hidden');

/// How wide the series pane is. The browser keeps it, not the configuration
/// file: it belongs to this screen, not to the tool.
const SIDE_MIN = 130;
const sideWidth = ref(Number(localStorage.getItem('qreview.side.width')) || 272);

function widen(by: number) {
  const room = Math.max(SIDE_MIN, window.innerWidth - 320);
  sideWidth.value = Math.min(Math.max(sideWidth.value + by, SIDE_MIN), room);
}

function keepWidth() {
  localStorage.setItem('qreview.side.width', String(Math.round(sideWidth.value)));
}
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
        void goToChange(next);
      }
      break;
    }
    case 'c':
      diffView.value?.commentHere();
      break;
    case 'v':
      diffView.value?.startRange();
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
      diffView.value?.clearPicked();
      break;
    case '/':
      if (!side.value) {
        toggleSide();
      }
      pane.value?.focusFilter();
      break;
    default:
      return;
  }

  // The key did something, so it must not also be typed. `c` opens a box and
  // puts the keyboard in it, and the letter would land in the box before the
  // reader wrote a word.
  event.preventDefault();
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

/// The tab names the tool, then the repository and the change being read.
///
/// The name comes first, so a tab is qreview at a glance among the other
/// tabs of the browser. The rest follows, because a reader opens one window
/// per review and a row of tabs that all say `qreview` says nothing about
/// which is which.
watch(
  [() => series.value?.repo.name, () => change.value?.subject],
  ([repo, subject]) => {
    document.title = ['qreview', repo, subject].filter(Boolean).join(' · ');
  },
  { immediate: true },
);

onMounted(() => {
  void goTo(null, review.load(parse(location.hash)));
  window.addEventListener('keydown', onKey);
  window.addEventListener('popstate', onPop);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey);
  window.removeEventListener('popstate', onPop);
});
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
      <img class="logo" src="/logo.svg" alt="" width="18" height="18" />
      <h1>qreview</h1>
      <p v-if="series" class="repo">{{ series.repo.remote ?? series.repo.root }}</p>

      <span class="bar-actions">
        <button
          type="button"
          class="chip"
          title="This change, as Markdown"
          :disabled="here === 0"
          @click="copy('change')"
        >
          Copy this change<span v-if="here" class="count"> · {{ here }}</span>
        </button>
        <button
          type="button"
          class="chip"
          title="The whole series"
          :disabled="total === 0"
          @click="copy('series')"
        >
          Copy the series<span v-if="total" class="count"> · {{ total }}</span>
        </button>
        <button
          type="button"
          class="chip"
          aria-label="Read the repository again"
          title="Read the repository again"
          :disabled="busy"
          @click="goTo(null, review.refresh())"
        >
          ⟳
        </button>
        <button type="button" class="chip" title="Keyboard shortcuts ( ? )" @click="helping = true">
          ?
        </button>
        <button type="button" class="chip" title="Preferences ( , )" @click="prefs = true">
          ⚙
        </button>
        <span v-if="copied" role="status" class="copied">copied</span>
        <span class="quiet version" :title="`qreview ${build}`">{{ version }}</span>
        <a
          v-if="release?.newer"
          class="newer"
          :href="release.url ?? undefined"
          :target="release.url ? '_blank' : undefined"
          rel="noreferrer"
          :title="`You are running ${version}`"
          >{{ release.latest }} available</a
        >
      </span>
    </header>

    <p v-if="error" role="alert" class="error">{{ error }}</p>

    <main
      v-if="series"
      class="body"
      :class="side ? '' : 'no-side'"
      :style="side ? { gridTemplateColumns: `${sideWidth}px 6px minmax(0, 1fr)` } : undefined"
    >
      <SidePane
        v-if="side"
        ref="pane"
        :series="series"
        :selected="changeKey"
        :files="files"
        :file-path="filePath"
        :busy="busy"
        :batch="batchSize"
        :loading-files="loadingFiles"
        :counts="countOf"
        :written="written"
        :in-file="inFile"
        @open-change="goToChange"
        @open-file="goToFile"
        @go="goToComment"
        @mark="review.markChange"
        @more="review.loadMore()"
      />

      <PaneSplit
        v-if="side"
        direction="vertical"
        label="Make the series pane wider or narrower"
        @move="widen"
        @done="keepWidth"
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
          :parents="change?.parents ?? []"
          @open="goToVersion"
          @fetch="review.fetchPatchSet"
        />
        <MergeBar
          v-if="onMerge"
          :base="against"
          :list="mergeList"
          @show-list="review.loadMergeList()"
        />
        <div class="diff-slot">
          <DiffView
            v-if="diff"
            ref="diffView"
            class="grow"
            :change-key="changeKey ?? ''"
            :diff="diff"
            :split="split"
            :wrap="wrap"
            :comments="comments"
            :stranded="stranded"
            :paths="paths"
            :current-sha="currentSha"
            :sets="patchSets"
            :placement="review.placement"
            :posted="posted.comments"
            :posted-placement="review.postedPlacement"
            :posted-stranded="postedStranded"
            :read-only="readingOlder"
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
          <!-- The file list is read first, and the diff on the screen is
               the one from before until the new one lands. -->
          <LoadingVeil :when="loadingDiff || loadingFiles" label="Reading the file" />
        </div>
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
