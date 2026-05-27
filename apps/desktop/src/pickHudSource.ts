import type { StatusSnapshot, VibePhase, VibeSource } from "./types";

const IN_PROGRESS: VibePhase[] = ["active", "waiting_user"];

function newestSession(
  snap: StatusSnapshot,
  pred: (phase: VibePhase) => boolean
): VibeSource | null {
  const candidates = snap.sessions.filter((s) => pred(s.phase));
  if (candidates.length === 0) return null;
  return candidates.sort(
    (a, b) =>
      new Date(b.last_activity_at).getTime() -
      new Date(a.last_activity_at).getTime()
  )[0].source;
}

function newestSourceByHealth(snap: StatusSnapshot): VibeSource | null {
  const sources: VibeSource[] = ["cursor", "claude_code", "codex"];
  let best: { source: VibeSource; at: number } | null = null;
  for (const source of sources) {
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
    newestSession(snap, (p) => IN_PROGRESS.includes(p)) ??
    newestSession(snap, () => true) ??
    newestSourceByHealth(snap) ??
    fallback
  );
}
