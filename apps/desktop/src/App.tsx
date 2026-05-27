import { useCallback, useEffect, useRef, useState } from "react";
import { useMainWindowAutoSize } from "./useMainWindowAutoSize";
import { pickHudSource } from "./pickHudSource";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { StatusSnapshot, VibePhase, VibeSource } from "./types";
import { SOURCE_LABELS } from "./types";

function dotClass(phase: VibePhase): string {
  switch (phase) {
    case "active":
      return "active";
    case "waiting_user":
      return "waiting";
    case "stopped":
      return "stopped";
    case "idle":
      return "idle";
    default:
      return "unknown";
  }
}

function sessionForSource(snap: StatusSnapshot, source: VibeSource) {
  return snap.sessions.find((s) => s.source === source);
}

function WizardApp() {
  const [busy, setBusy] = useState(false);
  const [lite, setLite] = useState(true);
  const [msg, setMsg] = useState<string[]>([]);

  const enable = async () => {
    setBusy(true);
    try {
      const result = await invoke<{ ok: boolean; messages: string[] }>("install_hooks_cmd", {});
      setMsg(result.messages);
      if (result.ok) {
        await invoke("set_lite_mode", { enabled: lite });
        await invoke("finish_first_run");
        const w = getCurrentWebviewWindow();
        await w.close();
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="wizard">
      <h1>欢迎使用 Vibe Monitor</h1>
      <p>
        一键启用对 Cursor、Claude Code、Codex 的本地监听。数据仅保存在本机，不会上传云端。
      </p>
      <label style={{ display: "flex", gap: 8, alignItems: "center", margin: "12px 0" }}>
        <input type="checkbox" checked={lite} onChange={(e) => setLite(e.target.checked)} />
        同时开启轻量模式（监视 transcript，hook 失败时仍能看到活动）
      </label>
      <button className="primary" disabled={busy} onClick={enable}>
        {busy ? "安装中…" : "启用监听"}
      </button>
      {msg.length > 0 && (
        <ul className="doctor">
          {msg.map((m, i) => (
            <li key={i}>{m}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

function MainApp() {
  const appRef = useRef<HTMLDivElement>(null);
  const [snap, setSnap] = useState<StatusSnapshot | null>(null);
  const [defaultSource, setDefaultSource] = useState<VibeSource>("cursor");

  const displaySource = pickHudSource(snap, defaultSource);

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<StatusSnapshot>("get_status");
      setSnap(s);
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const d = await invoke<string>("get_default_source");
        if (d === "cursor" || d === "claude_code" || d === "codex") {
          setDefaultSource(d);
        }
      } catch {
        /* ignore */
      }
    })();
  }, []);

  useEffect(() => {
    refresh();
    let es: EventSource | null = null;
    (async () => {
      try {
        const base = await invoke<string>("get_base_url");
        es = new EventSource(`${base}/api/stream`);
        es.onmessage = (ev) => {
          try {
            setSnap(JSON.parse(ev.data) as StatusSnapshot);
          } catch {
            /* ignore */
          }
        };
        es.onerror = () => refresh();
      } catch {
        const t = setInterval(refresh, 2000);
        return () => clearInterval(t);
      }
    })();
    return () => es?.close();
  }, [refresh]);

  const health = snap?.sources?.[displaySource];
  const session = snap ? sessionForSource(snap, displaySource) : undefined;
  const phase = session?.phase ?? health?.phase ?? "unknown";
  const phaseClass = dotClass(phase);

  useEffect(() => {
    const html = document.documentElement;
    const root = document.getElementById("root");
    const dragAttr = "data-tauri-drag-region";
    const onContextMenu = (e: Event) => e.preventDefault();

    document.body.classList.add("hud-mode");
    document.body.setAttribute(dragAttr, "deep");
    html.setAttribute(dragAttr, "deep");
    root?.setAttribute(dragAttr, "deep");
    document.body.addEventListener("contextmenu", onContextMenu);

    return () => {
      document.body.classList.remove("hud-mode");
      document.body.removeAttribute(dragAttr);
      html.removeAttribute(dragAttr);
      root?.removeAttribute(dragAttr);
      document.body.removeEventListener("contextmenu", onContextMenu);
    };
  }, []);

  useMainWindowAutoSize(appRef, [snap, defaultSource, displaySource, phase]);

  return (
    <div
      className={`app phase-${phaseClass}`}
      ref={appRef}
      data-tauri-drag-region="deep"
    >
      <div className="row">
        <span className={`dot ${dotClass(phase)}`} />
        <span className="label">{SOURCE_LABELS[displaySource]}</span>
      </div>
    </div>
  );
}

export default function App() {
  const [mode, setMode] = useState<"loading" | "wizard" | "hud">("loading");

  useEffect(() => {
    const win = getCurrentWebviewWindow();
    const label = win.label;

    if (label === "wizard") {
      setMode("wizard");
      return;
    }

    // HUD 浮窗（main）只展示 MainApp，向导仅在 wizard 窗口
    setMode("hud");

    let disposed = false;
    let unlisten: (() => void) | undefined;

    win
      .listen("first-run-complete", () => {
        if (!disposed) {
          setMode("hud");
          void win.show();
          void win.setFocus();
        }
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  if (mode === "loading") return null;
  if (mode === "wizard") return <WizardApp />;
  return <MainApp />;
}
