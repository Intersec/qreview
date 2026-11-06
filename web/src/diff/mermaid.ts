// Mermaid diagrams of a rendered file, drawn in the page.
//
// Mermaid draws in a browser only: it measures its text in the DOM. It lets
// HTML through in a label, and the page holds the session token. So each
// diagram is drawn in a frame of its own, with `sandbox="allow-scripts"`: an
// opaque origin, with no access to the page. A Content-Security-Policy in
// the frame lets no request out. See `roadmap/stack.md`, 2026-11-06.

/// The script of Mermaid, about 5 MB, fetched once and only for a file that
/// holds a diagram.
let script: Promise<string> | null = null;

function mermaid(): Promise<string> {
  script ??= import('mermaid/dist/mermaid.min.js?raw').then(({ default: text }) => {
    // The script goes inline in the frame, where `</script` would end it.
    if (/<\/?script/i.test(text)) {
      throw new Error('the Mermaid script cannot be put in a frame');
    }
    return text;
  });
  return script;
}

/// What a frame sends to the page.
interface Message {
  qreviewDiagram: string;
  height?: number;
  error?: string;
}

/// The frames that wait for their answer, by their window.
const waiting = new Map<Window, { key: string; done: (message: Message) => void }>();

window.addEventListener('message', (event: MessageEvent<Message>) => {
  // The window of a sandboxed frame is of another origin, and such a window
  // is not an `instanceof Window`: the map finds it by identity.
  const frame = waiting.get(event.source as Window);
  // A page that a diagram loads in the frame does not know the key.
  if (!frame || event.data?.qreviewDiagram !== frame.key) {
    return;
  }
  frame.done(event.data);
});

/// Draw each diagram under `host` in place of its code.
///
/// The code stays until the diagram is drawn, and stays for good if Mermaid
/// cannot read it, with the error under it. `resized` is called each time a
/// diagram takes its height.
export function draw(host: HTMLElement, resized: () => void) {
  const blocks = [...host.querySelectorAll<HTMLElement>('pre.md-mermaid')];
  if (blocks.length === 0) {
    return;
  }
  const dark = isDark();
  for (const pre of blocks) {
    void one(pre, dark, resized);
  }
}

async function one(pre: HTMLElement, dark: boolean, resized: () => void) {
  const source = pre.textContent ?? '';
  const holder = document.createElement('div');
  holder.className = 'md-diagram';
  // A Markdown code block carries its lines: the diagram keeps them.
  for (const name of ['data-lines', 'data-change', 'data-old']) {
    const value = pre.getAttribute(name);
    if (value !== null) {
      holder.setAttribute(name, value);
      pre.removeAttribute(name);
    }
  }
  pre.replaceWith(holder);
  holder.append(pre);

  let text: string;
  try {
    text = await mermaid();
  } catch (e) {
    fail(holder, e instanceof Error ? e.message : String(e));
    resized();
    return;
  }

  const key = crypto.randomUUID();
  const frame = document.createElement('iframe');
  frame.setAttribute('sandbox', 'allow-scripts');
  frame.setAttribute('tabindex', '-1');
  frame.setAttribute('aria-hidden', 'true');
  frame.title = 'Diagram';
  frame.className = 'md-diagram-frame';
  // Mermaid measures text, which fails in a frame that is not drawn: the
  // frame stays drawn, with no height, until the diagram is done.
  frame.style.height = '0';
  holder.prepend(frame);

  // The window of a frame stays the same when its document loads, so the
  // page knows it before the frame can answer.
  const inner = frame.contentWindow;
  if (!inner) {
    frame.remove();
    fail(holder, 'the frame of the diagram did not open');
    resized();
    return;
  }
  const finish = (message: Message) => {
    clearTimeout(timer);
    waiting.delete(inner);
    if (message.error !== undefined || !message.height) {
      frame.remove();
      fail(holder, message.error ?? 'Mermaid drew nothing');
    } else {
      frame.style.height = `${message.height}px`;
      pre.hidden = true;
      // The rows of the code are gone: the pane reads the block as a whole.
      pre.removeAttribute('data-code');
    }
    resized();
  };
  // A frame that stops before it can answer still leaves the code.
  const timer = setTimeout(
    () => finish({ qreviewDiagram: key, error: 'Mermaid did not answer' }),
    TIMEOUT,
  );
  waiting.set(inner, { key, done: finish });
  frame.srcdoc = page(text, source, key, dark);
}

/// How long a diagram may take before its code stays for good.
const TIMEOUT = 20_000;

function fail(holder: HTMLElement, error: string) {
  const note = document.createElement('p');
  note.className = 'md-diagram-error';
  note.textContent = `The diagram cannot be drawn: ${error}`;
  holder.append(note);
}

/// The document of a frame. Every string that comes from the file goes in
/// as JSON, with `<` escaped, so it cannot end the script.
function page(mermaid: string, source: string, key: string, dark: boolean): string {
  const nonce = crypto.randomUUID().replaceAll('-', '');
  const json = (value: unknown) => JSON.stringify(value).replaceAll('<', '\\u003c');
  const config = {
    startOnLoad: false,
    // Without this, Mermaid adds an error image to the body.
    suppressErrorRendering: true,
    // No click handler, and Mermaid cleans the labels.
    securityLevel: 'strict',
    theme: dark ? 'dark' : 'default',
    // A diagram can change its configuration in a directive. These keys
    // stay as set here.
    secure: [
      'secure',
      'securityLevel',
      'startOnLoad',
      'suppressErrorRendering',
      'maxTextSize',
      'maxEdges',
      'themeCSS',
      'themeVariables',
      'fontFamily',
      'altFontFamily',
    ],
  };
  return `<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-${nonce}'; style-src 'unsafe-inline'; img-src data:; font-src data:">
<style>html, body { margin: 0; background: transparent; overflow: hidden; }</style>
</head><body><div id="diagram"></div>
<script nonce="${nonce}">${mermaid}</script>
<script nonce="${nonce}">
(async () => {
  const send = (message) => parent.postMessage({ qreviewDiagram: ${json(key)}, ...message }, '*');
  try {
    mermaid.initialize(${json(config)});
    const { svg } = await mermaid.render('diagram-svg', ${json(source)});
    const box = document.getElementById('diagram');
    box.innerHTML = svg;
    // A link would load another page in the frame, where this policy does
    // not hold.
    for (const link of box.querySelectorAll('a')) {
      link.removeAttribute('href');
      link.removeAttribute('xlink:href');
    }
    send({ height: Math.ceil(box.getBoundingClientRect().height) });
  } catch (e) {
    send({ error: String((e && e.message) || e) });
  }
})();
</script></body></html>`;
}

/// The theme of the page: the one the reader chose, or the one of the
/// system.
function isDark(): boolean {
  const chosen = document.documentElement.dataset.theme;
  if (chosen === 'dark' || chosen === 'light') {
    return chosen === 'dark';
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}
