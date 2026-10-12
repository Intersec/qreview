// A fix shows once, when the agent is done, and not at every step.

import { describe, expect, it, vi } from 'vitest';
import { CATCH_UP_EVERY, Catchup } from './catchup';
import type { StoreEvent } from '@/api/types';

function event(kind: StoreEvent['kind'], extra: Partial<StoreEvent> = {}): StoreEvent {
  return { seq: 1, kind, author: 'agent', key: 'Ione', id: null, ...extra };
}

function page(now = () => 0) {
  const act = {
    current: () => 'Ione',
    followDrafts: vi.fn(async () => {}),
    refresh: vi.fn(async () => {}),
    reload: vi.fn(async () => {}),
    readWritten: vi.fn(async () => {}),
    told: vi.fn(),
  };
  return { act, held: new Catchup(act, false, now) };
}

describe('the catch-up of a page', () => {
  it('reloads at once when no fix runs', async () => {
    const { act, held } = page();

    await held.settle([event('reply')], false);

    expect(act.reload).toHaveBeenCalledTimes(1);
  });

  it('holds a fix, then refreshes once, the drafts first', async () => {
    const { act, held } = page();
    const order: string[] = [];
    act.followDrafts.mockImplementation(async () => {
      order.push('drafts');
    });
    act.refresh.mockImplementation(async () => {
      order.push('refresh');
    });
    const migrated = event('migrated', { from: 'a', to: 'b', fromKey: 'sha-a' });

    await held.settle([event('reply')], true);
    await held.settle([migrated, event('migrated')], true);
    await held.settle([event('reply')], true);
    expect(act.refresh).not.toHaveBeenCalled();
    expect(act.reload).not.toHaveBeenCalled();

    await held.settle([event('idle', { key: '' })], false);

    expect(order).toEqual(['drafts', 'drafts', 'refresh']);
    expect(act.reload).not.toHaveBeenCalled();
  });

  it('shows a long fix every ten minutes, and holds again', async () => {
    let clock = 0;
    const { act, held } = page(() => clock);

    await held.settle([event('reply')], true);
    clock = CATCH_UP_EVERY + 1;
    await held.settle([event('reply')], true);
    expect(act.reload).toHaveBeenCalledTimes(1);

    await held.settle([event('reply')], true);
    expect(act.reload).toHaveBeenCalledTimes(1);
  });

  it('leaves the writes of the reader and the idle alone', async () => {
    const { act, held } = page();

    await held.settle([event('comment', { author: 'reader' }), event('idle', { key: '' })], false);

    expect(act.reload).not.toHaveBeenCalled();
    expect(act.readWritten).not.toHaveBeenCalled();
  });

  it('tells the end of a fix once, after the page caught up', async () => {
    const { act, held } = page();
    act.refresh.mockImplementation(async () => {
      expect(act.told).not.toHaveBeenCalled();
    });

    await held.settle([event('fix', { author: 'reader' })], true);
    await held.settle([event('reply'), event('migrated')], true);
    expect(act.told).not.toHaveBeenCalled();
    await held.settle([event('idle', { key: '' })], false);
    await held.settle([], false);

    expect(act.told).toHaveBeenCalledTimes(1);
    expect(act.told).toHaveBeenCalledWith('fix');
  });

  it('tells an answer outside a fix, and not a write of the reader', async () => {
    const { act, held } = page();

    await held.settle([event('comment', { author: 'reader' })], false);
    expect(act.told).not.toHaveBeenCalled();
    await held.settle([event('reply')], false);

    expect(act.told).toHaveBeenCalledWith('answer');
  });

  it('reads the counts again for another change', async () => {
    const { act, held } = page();

    await held.settle([event('reply', { key: 'Itwo' })], false);

    expect(act.readWritten).toHaveBeenCalledTimes(1);
  });
});
