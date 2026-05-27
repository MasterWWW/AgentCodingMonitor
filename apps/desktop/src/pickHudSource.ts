import type { Session, StatusSnapshot, VibePhase, VibeSource } from "./types";

const IN_PROGRESS: VibePhase[] = ["active", "waiting_user"];
const NEAR_DUPLICATE_MS = 2000;

const SOURCE_ORDER: VibeSource[] = ["cursor", "claude_code", "codex"];

function sourceDisplayPriority(
  source: VibeSource,
  defaultSource: VibeSource
): number {
  if (source === defaultSource) return 0;
  const idx = SOURCE_ORDER.indexOf(source);
  return idx >= 0 ? idx + 1 : 99;
}

function isNearDuplicateGroup(sessions: Session[]): boolean {
  if (sessions.length < 2) return false;
  const times = sessions.map((s) => new Date(s.last_activity_at).getTime());
  const min = Math.min(...times);
  const max = Math.max(...times);
  return max - min <= NEAR_DUPLICATE_MS;
}

function pickSourceFromSessionGroup(
  sessions: Session[],
  defaultSource: VibeSource
): VibeSource {
  if (isNearDuplicateGroup(sessions)) {
    const sorted = [...sessions].sort((a, b) => {
      const pa = sourceDisplayPriority(a.source, defaultSource);
      const pb = sourceDisplayPriority(b.source, defaultSource);
      if (pa !== pb) return pa - pb;
      return (
        new Date(b.last_activity_at).getTime() -
        new Date(a.last_activity_at).getTime()
      );
    });
    return sorted[0].source;
  }
  const newest = [...sessions].sort(
    (a, b) =>
      new Date(b.last_activity_at).getTime() -
      new Date(a.last_activity_at).getTime()
  )[0];
  return newest.source;
}

function newestSession(
  snap: StatusSnapshot,
  defaultSource: VibeSource,
  pred: (phase: VibePhase) => boolean
): VibeSource | null {
  const candidates = snap.sessions.filter((s) => pred(s.phase));
  if (candidates.length === 0) return null;

  const groups = new Map<string, Session[]>();
  for (const s of candidates) {
    const list = groups.get(s.session_id) ?? [];
    list.push(s);
    groups.set(s.session_id, list);
  }

  let best: { source: VibeSource; at: number } | null = null;
  for (const sessions of groups.values()) {
    const source = pickSourceFromSessionGroup(sessions, defaultSource);
    const at = Math.max(
      ...sessions.map((s) => new Date(s.last_activity_at).getTime())
    );
    if (!best || at > best.at) {
      best = { source, at };
    }
  }
  return best?.source ?? null;
}

function newestSourceByHealth(snap: StatusSnapshot): VibeSource | null {
  let best: { source: VibeSource; at: number } | null = null;
  for (const source of SOURCE_ORDER) {
    const last = snap.sources?.[source]?.last_seen;
    if (!last) continue;
    const at = new Date(last).getTime();
    if (!best || at > best.at) {
      best = { source, at };
    }
  }
  return best?.source ?? null;
}

/** HUD label: in-progress → any recent session → health last_seen → default. */
export function pickHudSource(
  snap: StatusSnapshot | null,
  fallback: VibeSource
): VibeSource {
  if (!snap) return fallback;
  return (
    newestSession(snap, fallback, (p) => IN_PROGRESS.includes(p)) ??
    newestSession(snap, fallback, () => true) ??
    newestSourceByHealth(snap) ??
    fallback
  );
}
