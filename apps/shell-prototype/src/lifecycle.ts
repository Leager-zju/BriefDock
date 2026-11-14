export type Root = 'news' | 'hongguo';
export type Section = 'content' | 'modules' | 'settings';
export type View = { root: Root; section: Section; visible: boolean; generation: number };
export function active(view: View, root: Root): boolean {
  return view.visible && view.section === 'content' && view.root === root;
}
export function accepts(current: View, request: View): boolean {
  return current.generation === request.generation && active(current, request.root);
}
export function toggleSection(current: Section, next: Section): Section {
  return current === next ? 'content' : next;
}
export function formatTime(seconds: number): string {
  const safe = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  return `${Math.floor(safe / 60).toString().padStart(2, '0')}:${(safe % 60).toString().padStart(2, '0')}`;
}
