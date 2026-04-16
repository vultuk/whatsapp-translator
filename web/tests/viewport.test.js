import test from 'node:test';
import assert from 'node:assert/strict';
import { calculateViewportLayout } from '../public/viewport.js';

test('calculateViewportLayout keeps CSS large viewport sizing when keyboard is closed', () => {
  assert.deepEqual(
    calculateViewportLayout({
      innerHeight: 780,
      viewportHeight: 704,
      viewportOffsetTop: 0,
    }),
    {
      keyboardOffset: 76,
      keyboardOpen: false,
      effectiveHeight: null,
    },
  );
});

test('calculateViewportLayout switches to the visual viewport when keyboard is open', () => {
  assert.deepEqual(
    calculateViewportLayout({
      innerHeight: 780,
      viewportHeight: 420,
      viewportOffsetTop: 0,
    }),
    {
      keyboardOffset: 360,
      keyboardOpen: true,
      effectiveHeight: 420,
    },
  );
});

test('calculateViewportLayout includes visual viewport offset when present', () => {
  assert.deepEqual(
    calculateViewportLayout({
      innerHeight: 780,
      viewportHeight: 430,
      viewportOffsetTop: 18,
    }),
    {
      keyboardOffset: 332,
      keyboardOpen: true,
      effectiveHeight: 448,
    },
  );
});
