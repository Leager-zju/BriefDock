import { test } from 'node:test';
import assert from 'node:assert/strict';
import { accepts, active, toggleSection, moduleTarget, formatTime, type View } from '../src/lifecycle.ts';
const news: View = { root: 'news', section: 'content', visible: true, generation: 1 };
test('A→B→A rejects old results even when the root matches again', () => {
  assert.equal(accepts({ ...news, generation: 3 }, news), false);
  assert.equal(accepts(news, news), true);
  assert.equal(accepts({ ...news, root: 'hongguo' }, news), false);
});
test('settings, modules and hidden windows silence content', () => {
  for (const section of ['settings', 'modules'] as const) assert.equal(active({ ...news, section }, 'news'), false);
  assert.equal(active({ ...news, visible: false }, 'news'), false);
  assert.equal(active(news, 'hongguo'), false);
});
test('global buttons toggle closed and media times stay readable', () => {
  assert.equal(toggleSection('modules', 'modules'), 'content');
  assert.equal(toggleSection('settings', 'modules'), 'modules');
  assert.equal(formatTime(65.9), '01:05');
  assert.equal(formatTime(NaN), '00:00');
});
test('module back returns to the page that opened it, including settings', () => {
  assert.equal(moduleTarget('content', 'settings'), 'modules');
  assert.equal(moduleTarget('modules', 'content'), 'content');
  assert.equal(moduleTarget('modules', 'settings'), 'settings');
});
