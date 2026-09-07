// Where the reader is, and how it rides in the URL.
//
// The browser owns the history of a review: back and forward walk the places
// a reader has been, and a reload comes back to the last one. The place goes
// in the fragment, because the query already carries the session token and
// the server serves one page under one address.

export interface Place {
  /// The key of the change being read.
  change: string;
  /// The file open in the pane, when one is.
  file?: string;
  /// The patch set being read. Absent is the newest.
  ps?: number;
  /// What that patch set is read against. Absent is the default base.
  base?: string;
}

/// The fragment of a place, `#change=I8f3a&file=src/net.blk&ps=2`.
export function format(place: Place): string {
  const parts = [`change=${encode(place.change)}`];

  if (place.file) {
    parts.push(`file=${encode(place.file)}`);
  }
  if (place.ps !== undefined) {
    parts.push(`ps=${place.ps}`);
  }
  if (place.base) {
    parts.push(`base=${encode(place.base)}`);
  }
  return `#${parts.join('&')}`;
}

/// The place a fragment names, or `null` when it names none.
export function parse(fragment: string): Place | null {
  const fields = new URLSearchParams(fragment.replace(/^#/, ''));
  const change = fields.get('change');
  if (!change) {
    return null;
  }
  const ps = Number(fields.get('ps'));

  return {
    change,
    file: fields.get('file') ?? undefined,
    ps: Number.isInteger(ps) && ps > 0 ? ps : undefined,
    base: fields.get('base') ?? undefined,
  };
}

/// Whether two places are the same one. What the fragment says is all that
/// a history entry holds, so that is what is compared.
export function same(one: Place | null, other: Place | null): boolean {
  if (one === null || other === null) {
    return one === other;
  }
  return format(one) === format(other);
}

/// A path reads better with its slashes, and `ps:2` with its colon. Neither
/// one means anything in a fragment. Everything else is escaped, so a file
/// whose name carries a `&`, a `+` or a space comes back whole.
function encode(value: string): string {
  return encodeURIComponent(value).replaceAll('%2F', '/').replaceAll('%3A', ':');
}
