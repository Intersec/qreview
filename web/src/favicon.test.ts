import { describe, expect, it } from 'vitest';
import { badge, signalOf } from './favicon';

const calm = { stopped: false, left: false, working: false, agent: false };

describe('signalOf', () => {
  it('draws no dot when no agent ever came', () => {
    expect(signalOf(calm)).toBeNull();
  });

  it('puts the most urgent state first', () => {
    expect(signalOf({ ...calm, agent: true })).toBe('connected');
    expect(signalOf({ ...calm, left: true })).toBe('left');
    expect(signalOf({ ...calm, agent: true, working: true })).toBe('working');
    expect(signalOf({ stopped: true, left: true, working: true, agent: true })).toBe('stopped');
  });
});

describe('badge', () => {
  it('wraps the logo and adds the dot of the signal', () => {
    const svg = badge(
      '<?xml version="1.0"?><svg viewBox="0 0 32 32"><path d="M0 0"/></svg>',
      'left',
    );

    expect(svg).not.toContain('<?xml');
    expect(svg).toContain('<path d="M0 0"/>');
    expect(svg).toContain('fill="#f29900"');
  });
});
