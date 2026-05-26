export type ChapterOffset = -1 | 1;

export type ChapterLike = {
  path: string;
};

export type GesturePoint = {
  x: number;
  y: number;
  timeMs: number;
};

type SwipeOptions = {
  minHorizontalDistance?: number;
  maxVerticalRatio?: number;
  maxGestureMs?: number;
};

type WheelOptions = {
  minHorizontalDistance?: number;
  maxVerticalRatio?: number;
};

const defaultSwipeOptions: Required<SwipeOptions> = {
  minHorizontalDistance: 96,
  maxVerticalRatio: 0.35,
  maxGestureMs: 900,
};

const defaultWheelOptions: Required<WheelOptions> = {
  minHorizontalDistance: 360,
  maxVerticalRatio: 0.25,
};

export function targetChapterPath(
  chapters: readonly ChapterLike[],
  selectedPath: string | undefined,
  offset: ChapterOffset,
): string | undefined {
  if (chapters.length === 0) return undefined;
  const selectedIndex = chapters.findIndex(chapter => chapter.path === selectedPath);
  const currentIndex = selectedIndex >= 0 ? selectedIndex : 0;
  return chapters[currentIndex + offset]?.path;
}

export function interpretHorizontalSwipe(
  start: GesturePoint,
  end: GesturePoint,
  options: SwipeOptions = {},
): ChapterOffset | undefined {
  const resolved = { ...defaultSwipeOptions, ...options };
  const deltaX = end.x - start.x;
  const deltaY = end.y - start.y;
  const elapsedMs = end.timeMs - start.timeMs;
  const absX = Math.abs(deltaX);
  const absY = Math.abs(deltaY);

  if (elapsedMs < 0 || elapsedMs > resolved.maxGestureMs) return undefined;
  if (absX < resolved.minHorizontalDistance) return undefined;
  if (absY > absX * resolved.maxVerticalRatio) return undefined;

  return deltaX < 0 ? 1 : -1;
}

export function interpretHorizontalWheel(
  deltaX: number,
  deltaY: number,
  options: WheelOptions = {},
): ChapterOffset | undefined {
  const resolved = { ...defaultWheelOptions, ...options };
  const absX = Math.abs(deltaX);
  const absY = Math.abs(deltaY);

  if (absX < resolved.minHorizontalDistance) return undefined;
  if (absY > absX * resolved.maxVerticalRatio) return undefined;

  return deltaX > 0 ? 1 : -1;
}
