// Tell a reader who looks elsewhere that the agent is done.
//
// A fix takes minutes, and an answer comes when it comes, so the reader goes
// to another window. The browser's own notification reaches them there. The
// browser asks for leave once, and its answer is the only setting: a reader
// who refused is told by the tab title instead.

import { ref } from 'vue';

/// Something happened while the tab was hidden. The tab title says so until
/// the reader comes back.
export const unseen = ref(false);

if (typeof document !== 'undefined') {
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) {
      unseen.value = false;
    }
  });
}

/// Ask for leave to notify. A browser asks only from a click, so this runs
/// on the clicks that start work for the agent.
export function askLeave() {
  if ('Notification' in window && Notification.permission === 'default') {
    void Notification.requestPermission().catch(() => {});
  }
}

/// Tell the reader, unless they are looking at the page already.
export function tell(title: string, body: string) {
  if (!document.hidden) {
    return;
  }
  unseen.value = true;
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
