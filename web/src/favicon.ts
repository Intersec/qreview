// The state of the review, on the icon of the tab.
//
// A reader keeps the review in one tab among many, and the top bar is out
// of sight there. The icon is not: a dot in its corner says whether the
// agent listens, works, or left, and whether the server stopped.

/// What the dot says, the most urgent first. None draws the plain logo.
export type Signal = 'stopped' | 'left' | 'working' | 'connected' | null;

/// Fixed colors, not the theme: the tab bar belongs to the browser.
const COLORS: Record<Exclude<Signal, null>, string> = {
  stopped: '#d93025',
  left: '#f29900',
  working: '#1a73e8',
  connected: '#1e8e3e',
};

export function signalOf(state: {
  stopped: boolean;
  left: boolean;
  working: boolean;
  agent: boolean;
}): Signal {
  if (state.stopped) {
    return 'stopped';
  }
  if (state.working) {
    return 'working';
  }
  if (state.left) {
    return 'left';
  }
  return state.agent ? 'connected' : null;
}

/// The logo with the dot of a signal, as an SVG document. A white ring
/// keeps the dot apart from the logo and from a dark tab bar.
export function badge(logo: string, signal: Exclude<Signal, null>): string {
  const inner = logo.replace(/<\?xml[^>]*\?>/, '');

  return (
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">' +
    inner +
    '<circle cx="24" cy="24" r="8" fill="#fff"/>' +
    `<circle cx="24" cy="24" r="6" fill="${COLORS[signal]}"/>` +
    '</svg>'
  );
}

let logo: Promise<string> | null = null;
let shown: Signal = null;

/// Put the dot of a signal on the icon of the tab.
export async function show(signal: Signal) {
  shown = signal;
  // Read while the server runs: a stopped server serves no logo.
  logo ??= fetch('/logo.svg')
    .then((answer) => answer.text())
    .catch(() => '');
  const link = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
  if (!link) {
    return;
  }
  if (signal === null) {
    link.href = '/logo.svg';
    return;
  }
  const text = await logo;
  if (shown === signal) {
    link.href = `data:image/svg+xml,${encodeURIComponent(badge(text, signal))}`;
  }
}
