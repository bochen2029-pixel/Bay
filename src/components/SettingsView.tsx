// Settings view. Covers the General / Capture / LLM / Advanced
// sections per SPEC §5.3 + PROMPTS.md I-11. Persistence and hotkey
// reconfiguration are backend-driven; the frontend dispatches partial
// patches and mirrors the canonical Settings the backend returns.
//
// Write-only api_key flow: the key is POSTed via set_llm_api_key and
// never echoes back — `settings.llm.has_api_key` is the only signal
// the frontend ever sees that a key exists.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";

import { Settings, Tier } from "../domain";
import { useStore } from "../store";

export function SettingsView() {
  const settings = useStore((s) => s.settings);
  if (!settings) return <div className="settings">Loading…</div>;

  return (
    <div className="settings">
      <h2 className="settings-title">Settings</h2>
      <GeneralSection settings={settings} />
      <CaptureSection settings={settings} />
      <LlmSection settings={settings} />
      <AdvancedSection />
    </div>
  );
}

// ── sections ─────────────────────────────────────────────────────

function GeneralSection({ settings }: { settings: Settings }) {
  return (
    <section className="settings-section">
      <h3>General</h3>

      <FieldRow label="Quick-capture hotkey">
        <HotkeyInput
          value={settings.hotkey}
          onChange={(v) => void persistPatch({ hotkey: v })}
        />
      </FieldRow>

      {(["inbox", "A", "B", "C"] as Tier[]).map((tier) => (
        <FieldRow key={tier} label={`Staleness · ${tier}`}>
          <NullableDayInput
            value={daysFor(settings, tier)}
            onChange={(v) => void persistPatch(stalenessPatch(tier, v))}
          />
        </FieldRow>
      ))}

      <FieldRow label="Event log">
        <button type="button" onClick={handleExport}>
          Export event log…
        </button>
      </FieldRow>
    </section>
  );
}

type LanStatus = {
  enabled: boolean;
  url: string | null;
  qr_svg: string | null;
  port: number | null;
};

function CaptureSection({ settings }: { settings: Settings }) {
  const [status, setStatus] = useState<LanStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<LanStatus>("get_lan_capture_status")
      .then(setStatus)
      .catch((err) => setError(String(err)));
  }, []);

  async function toggle(enabled: boolean) {
    setBusy(true);
    setError(null);
    try {
      // Persist the preference first so the server reads the current
      // port / secret when enabled.
      await persistPatch({ lan_capture_enabled: enabled });
      const next = await invoke<LanStatus>("toggle_lan_capture", { enabled });
      setStatus(next);
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
      // Roll back the preference on failure.
      await persistPatch({ lan_capture_enabled: !enabled });
    }
    setBusy(false);
  }

  return (
    <section className="settings-section">
      <h3>Capture (LAN)</h3>
      <p className="settings-section-note">
        When enabled, a tiny HTTP server binds on the LAN so you can
        submit captures from another device. LAN-trust by default;
        enable the shared secret for a belt-and-suspenders check.
      </p>

      <FieldRow label="Enabled">
        <input
          type="checkbox"
          checked={settings.lan_capture_enabled}
          disabled={busy}
          onChange={(e) => void toggle(e.target.checked)}
        />
      </FieldRow>

      <FieldRow label="Port">
        <input
          type="number"
          min={1024}
          max={65535}
          value={settings.lan_capture_port}
          disabled={busy || settings.lan_capture_enabled}
          onChange={(e) =>
            void persistPatch({ lan_capture_port: Number(e.target.value) })
          }
        />
        {settings.lan_capture_enabled ? (
          <span className="is-dim">disable to change</span>
        ) : null}
      </FieldRow>

      {status?.enabled && status.url ? (
        <FieldRow label="Capture URL">
          <span className="settings-capture-url">
            <code>{status.url}</code>
          </span>
        </FieldRow>
      ) : null}

      {status?.enabled && status.qr_svg ? (
        <FieldRow label="Scan">
          <div
            className="settings-qr"
            dangerouslySetInnerHTML={{ __html: status.qr_svg }}
          />
        </FieldRow>
      ) : null}

      {error ? <div className="modal-error">{error}</div> : null}

      <details className="settings-advanced">
        <summary>Advanced: shared secret</summary>
        <FieldRow label="Shared secret">
          <input
            type="text"
            value={settings.lan_capture_shared_secret ?? ""}
            placeholder="(none)"
            disabled={settings.lan_capture_enabled}
            onChange={(e) =>
              void persistPatch({
                lan_capture_shared_secret: e.target.value || null,
              })
            }
          />
        </FieldRow>
      </details>
    </section>
  );
}

function LlmSection({ settings }: { settings: Settings }) {
  const [pendingKey, setPendingKey] = useState("");
  const [busy, setBusy] = useState(false);

  async function saveApiKey(clear: boolean) {
    setBusy(true);
    try {
      const raw = await invoke<unknown>("set_llm_api_key", {
        args: { api_key: clear ? "" : pendingKey.trim() },
      });
      const next = Settings.parse(raw);
      useStore.setState({ settings: next });
      setPendingKey("");
    } catch (err) {
      console.error("set_llm_api_key failed:", err);
    }
    setBusy(false);
  }

  return (
    <section className="settings-section">
      <h3>LLM</h3>
      <p className="settings-section-note">
        Test-connection and Analyze land in I-13 / I-14. Fields persist
        now so they're ready when the client wires in.
      </p>

      <FieldRow label="Base URL">
        <input
          type="text"
          value={settings.llm.base_url}
          onChange={(e) =>
            void persistPatch({ llm: { base_url: e.target.value } })
          }
        />
      </FieldRow>
      <FieldRow label="Model">
        <input
          type="text"
          value={settings.llm.model}
          onChange={(e) => void persistPatch({ llm: { model: e.target.value } })}
        />
      </FieldRow>
      <FieldRow label="Timeout (ms)">
        <input
          type="number"
          min={1000}
          max={600000}
          step={1000}
          value={settings.llm.timeout_ms}
          onChange={(e) =>
            void persistPatch({ llm: { timeout_ms: Number(e.target.value) } })
          }
        />
      </FieldRow>
      <FieldRow label="Analyze window (days)">
        <input
          type="number"
          min={1}
          max={365}
          value={settings.analyze_window_days}
          onChange={(e) =>
            void persistPatch({ analyze_window_days: Number(e.target.value) })
          }
        />
      </FieldRow>

      <FieldRow label="API key">
        {settings.llm.has_api_key ? (
          <span className="settings-key-stored">
            ●●●● stored in keychain
          </span>
        ) : (
          <span className="settings-key-absent">not set</span>
        )}
      </FieldRow>
      <FieldRow label={settings.llm.has_api_key ? "Replace" : "Set"}>
        <input
          type="password"
          value={pendingKey}
          placeholder="paste API key"
          onChange={(e) => setPendingKey(e.target.value)}
          disabled={busy}
        />
        <button
          type="button"
          disabled={busy || pendingKey.trim().length === 0}
          onClick={() => void saveApiKey(false)}
        >
          Save to keychain
        </button>
        {settings.llm.has_api_key ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              if (confirm("Remove the stored API key?")) void saveApiKey(true);
            }}
          >
            Clear
          </button>
        ) : null}
      </FieldRow>
    </section>
  );
}

function AdvancedSection() {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function handleRebuild() {
    const ok = confirm(
      "Rebuild projection from the event log?\n\n" +
        "This drops the items table and replays every event. The event " +
        "log is not touched. Use if the projection has drifted from " +
        "the log (e.g., after a manual DB edit).",
    );
    if (!ok) return;
    setBusy(true);
    setMessage(null);
    try {
      const raw = await invoke<unknown>("rebuild_projection");
      const parsed = raw as { items_affected: number };
      setMessage(`Rebuilt — ${parsed.items_affected} items in the projection.`);
    } catch (err) {
      setMessage(`Rebuild failed: ${String(err)}`);
    }
    setBusy(false);
  }

  return (
    <section className="settings-section">
      <h3>Advanced</h3>
      <p className="settings-section-note">
        Dangerous tools that are safe by construction but surprising.
        Use when needed.
      </p>
      <FieldRow label="Projection">
        <button type="button" onClick={handleRebuild} disabled={busy}>
          Rebuild projection from event log
        </button>
      </FieldRow>
      {message ? <div className="settings-message">{message}</div> : null}
    </section>
  );
}

// ── helpers ──────────────────────────────────────────────────────

function FieldRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-label">{label}</div>
      <div className="settings-control">{children}</div>
    </div>
  );
}

type PatchShape = {
  hotkey?: string;
  staleness_inbox_days?: number | null;
  staleness_a_days?: number | null;
  staleness_b_days?: number | null;
  staleness_c_days?: number | null;
  lan_capture_enabled?: boolean;
  lan_capture_port?: number;
  lan_capture_shared_secret?: string | null;
  llm?: { base_url?: string; model?: string; timeout_ms?: number };
  analyze_window_days?: number;
};

async function persistPatch(patch: PatchShape) {
  try {
    const raw = await invoke<unknown>("update_settings", { patch });
    const next = Settings.parse(raw);
    useStore.setState({ settings: next });
  } catch (err) {
    console.error("update_settings failed:", err);
  }
}

function stalenessPatch(tier: Tier, value: number | null): PatchShape {
  switch (tier) {
    case "inbox":
      return { staleness_inbox_days: value };
    case "A":
      return { staleness_a_days: value };
    case "B":
      return { staleness_b_days: value };
    case "C":
      return { staleness_c_days: value };
  }
}

function daysFor(settings: Settings, tier: Tier): number | null {
  switch (tier) {
    case "inbox":
      return settings.staleness_inbox_days;
    case "A":
      return settings.staleness_a_days;
    case "B":
      return settings.staleness_b_days;
    case "C":
      return settings.staleness_c_days;
  }
}

async function handleExport() {
  try {
    const path = await save({
      defaultPath: "bay-events.jsonl",
      filters: [{ name: "JSON Lines", extensions: ["jsonl"] }],
    });
    if (!path) return;
    const raw = await invoke<unknown>("export_events", { path });
    const result = raw as { events_written: number; path: string };
    alert(
      `Exported ${result.events_written} events to\n${result.path}`,
    );
  } catch (err) {
    alert(`Export failed: ${String(err)}`);
  }
}

// ── sub-widgets ──────────────────────────────────────────────────

function NullableDayInput({
  value,
  onChange,
}: {
  value: number | null;
  onChange: (v: number | null) => void;
}) {
  return (
    <span className="settings-nullable-days">
      <label>
        <input
          type="checkbox"
          checked={value !== null}
          onChange={(e) =>
            onChange(e.target.checked ? (value ?? 7) : null)
          }
        />
        enabled
      </label>
      {value !== null ? (
        <>
          <input
            type="number"
            min={1}
            max={3650}
            value={value}
            onChange={(e) => onChange(Number(e.target.value))}
          />
          <span>days</span>
        </>
      ) : (
        <span className="is-dim">disabled</span>
      )}
    </span>
  );
}

function HotkeyInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (s: string) => void;
}) {
  const [capturing, setCapturing] = useState(false);
  useEffect(() => {
    if (!capturing) return;
    function handler(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setCapturing(false);
        return;
      }
      const mods: string[] = [];
      if (e.ctrlKey) mods.push("Ctrl");
      if (e.altKey) mods.push("Alt");
      if (e.shiftKey) mods.push("Shift");
      if (e.metaKey) mods.push("Super");
      if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
      if (mods.length === 0) return;
      e.preventDefault();
      const keyStr =
        e.key.length === 1
          ? e.key.toUpperCase()
          : e.key.replace(/^Arrow/, "");
      onChange(`${mods.join("+")}+${keyStr}`);
      setCapturing(false);
    }
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [capturing, onChange]);

  return (
    <button
      type="button"
      className="settings-hotkey-btn"
      onClick={() => setCapturing((v) => !v)}
    >
      {capturing ? "Press key combo… (Esc cancels)" : value}
    </button>
  );
}
