import { interpretHorizontalSwipe, interpretHorizontalWheel, targetChapterPath } from './readerNavigation.js';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

const chapters = [
  { path: 'chapter-1.md' },
  { path: 'chapter-2.md' },
  { path: 'chapter-3.md' },
];

assert(targetChapterPath(chapters, 'chapter-2.md', 1) === 'chapter-3.md', 'next chapter selected');
assert(targetChapterPath(chapters, 'chapter-2.md', -1) === 'chapter-1.md', 'previous chapter selected');
assert(targetChapterPath(chapters, 'chapter-1.md', -1) === undefined, 'first chapter boundary is a no-op');
assert(targetChapterPath(chapters, 'chapter-3.md', 1) === undefined, 'last chapter boundary is a no-op');
assert(targetChapterPath(chapters, undefined, 1) === 'chapter-2.md', 'missing selection falls back to first chapter');

assert(
  interpretHorizontalSwipe({ x: 260, y: 40, timeMs: 0 }, { x: 120, y: 48, timeMs: 180 }) === 1,
  'left swipe goes to next chapter',
);
assert(
  interpretHorizontalSwipe({ x: 120, y: 40, timeMs: 0 }, { x: 260, y: 45, timeMs: 180 }) === -1,
  'right swipe goes to previous chapter',
);
assert(
  interpretHorizontalSwipe({ x: 120, y: 40, timeMs: 0 }, { x: 155, y: 42, timeMs: 180 }) === undefined,
  'short swipe is ignored',
);
assert(
  interpretHorizontalSwipe({ x: 120, y: 40, timeMs: 0 }, { x: 230, y: 150, timeMs: 180 }) === undefined,
  'mostly vertical swipe is ignored',
);
assert(
  interpretHorizontalSwipe({ x: 260, y: 40, timeMs: 0 }, { x: 120, y: 48, timeMs: 1600 }) === undefined,
  'stale drag is ignored',
);

assert(interpretHorizontalWheel(420, 20) === 1, 'horizontal wheel right goes to next chapter');
assert(interpretHorizontalWheel(-420, 20) === -1, 'horizontal wheel left goes to previous chapter');
assert(interpretHorizontalWheel(180, 0) === undefined, 'small horizontal wheel movement is ignored');
assert(interpretHorizontalWheel(420, 220) === undefined, 'diagonal/vertical wheel movement is ignored');
