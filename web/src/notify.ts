// Tell a reader who looks elsewhere that the agent is done.
//
// A fix takes minutes, and an answer comes when it comes, so the reader goes
// to another window. The browser's own notification reaches them there, and
// the tab title says it too: a notification goes away, a tab stays. The
// browser asks for leave once, and its answer is the only setting.

import { computed, ref, watch } from 'vue';

/// The agent finished while the reader looked elsewhere: their turn, until
/// the page has the focus again.
export const yourTurn = ref(false);

/// The questions of the agent that wait for the reader. Looking at the page
/// answers none of them, so the turn stays theirs while one is open.
export const questions = ref(0);

/// The symbol that turns in the title while it is the reader's turn, the
/// one Terraforming Mars spins for a player whose turn it is.
const TURN = '◑◒◐◓';
const frame = ref(0);
let spin: ReturnType<typeof setInterval> | undefined;

const turn = computed(() => yourTurn.value || questions.value > 0);

/// What the title starts with for the turn: a symbol that turns, so a tab
/// among many catches the eye.
export const turnMark = computed(() => (turn.value ? TURN[frame.value] : ''));

// A hidden tab runs its timers once a second at most: one step a second.
watch(turn, (now) => {
  if (now) {
    spin ??= setInterval(() => {
      frame.value = (frame.value + 1) % TURN.length;
    }, 1000);
  } else {
    clearInterval(spin);
    spin = undefined;
  }
});

function endTurn() {
  yourTurn.value = false;
}

if (typeof window !== 'undefined') {
  window.addEventListener('focus', endTurn);
}

/// Ask for leave to notify. A browser asks only from a click, so this runs
/// on the clicks that start work for the agent.
export function askLeave() {
  if ('Notification' in window && Notification.permission === 'default') {
    void Notification.requestPermission().catch(() => {});
  }
}

/// Tell the reader, unless the page has their attention already. A tab
/// on the screen behind another window does not.
export function tell(title: string, body: string) {
  if (document.hasFocus()) {
    return;
  }
  yourTurn.value = true;

  if (!('Notification' in window) || Notification.permission !== 'granted') {
    return;
  }
  // One tag: a second word of the agent replaces the first, not stacks.
  const note = new Notification(title, { body, tag: 'qreview', icon: '/logo.svg' });
  note.onclick = () => {
    window.focus();
    note.close();
  };
}
