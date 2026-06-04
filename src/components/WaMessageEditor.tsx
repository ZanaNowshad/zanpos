import { useState, useRef, useCallback } from "react";
import {
  type WaLine,
  type WaLang,
  type WaFormat,
  loadWaFormat,
  saveWaFormat,
  makeDefaultEnLines,
  makeDefaultArLines,
  buildSampleMessage,
  WA_VARIABLES,
} from "../utils/waMessageFormat";

// ── Line row ──────────────────────────────────────────────────────────────────

interface LineRowProps {
  line: WaLine;
  lang: WaLang;
  index: number;
  total: number;
  focusedLineId: string | null;
  onFocus: (id: string) => void;
  onChange: (id: string, template: string) => void;
  onToggle: (id: string) => void;
  onMove: (id: string, dir: -1 | 1) => void;
  onDelete: (id: string) => void;
}

function LineRow({
  line, lang, index, total, focusedLineId,
  onFocus, onChange, onToggle, onMove, onDelete,
}: LineRowProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  const isRtl = lang === "ar";
  const isFocused = focusedLineId === line.id;

  return (
    <div className={`wame-line${!line.enabled ? " wame-line-disabled" : ""}${isFocused ? " wame-line-focused" : ""}`}>
      {/* Toggle */}
      <button
        className={`wame-toggle${line.enabled ? " wame-toggle-on" : ""}`}
        onClick={() => onToggle(line.id)}
        title={line.enabled ? "Enabled — click to disable" : "Disabled — click to enable"}
      >
        {line.enabled ? "●" : "○"}
      </button>

      {/* Content */}
      <div className="wame-line-content">
        {line.type === "blank" && (
          <span className="wame-line-blank">── blank line ──</span>
        )}
        {line.type === "items" && (
          <span className="wame-line-items">📦 items list (auto-generated)</span>
        )}
        {line.type === "text" && (
          <input
            ref={inputRef}
            className={`wame-line-input${isRtl ? " wame-rtl" : ""}`}
            value={line.template}
            onChange={e => onChange(line.id, e.target.value)}
            onFocus={() => onFocus(line.id)}
            placeholder={isRtl ? "نص السطر…" : "Line text… use {variable_name}"}
            dir={isRtl ? "rtl" : "ltr"}
          />
        )}
      </div>

      {/* Controls */}
      <div className="wame-line-controls">
        <button
          className="wame-ctrl-btn"
          onClick={() => onMove(line.id, -1)}
          disabled={index === 0}
          title="Move up"
        >↑</button>
        <button
          className="wame-ctrl-btn"
          onClick={() => onMove(line.id, 1)}
          disabled={index === total - 1}
          title="Move down"
        >↓</button>
        <button
          className="wame-ctrl-btn wame-ctrl-del"
          onClick={() => onDelete(line.id)}
          title="Delete line"
        >✕</button>
      </div>
    </div>
  );
}

// ── Main editor ───────────────────────────────────────────────────────────────

interface WaMessageEditorProps {
  /** Loader for the format this editor edits. Defaults to the delivery format. */
  load?: () => WaFormat;
  /** Persister for the edited format. Defaults to the delivery format. */
  save?: (fmt: WaFormat) => void;
  /** Default English lines used by "Reset". Defaults to the delivery defaults. */
  makeDefaultEn?: () => WaLine[];
  /** Default Arabic lines used by "Reset". Defaults to the delivery defaults. */
  makeDefaultAr?: () => WaLine[];
}

export default function WaMessageEditor({
  load          = loadWaFormat,
  save          = saveWaFormat,
  makeDefaultEn = makeDefaultEnLines,
  makeDefaultAr = makeDefaultArLines,
}: WaMessageEditorProps = {}) {
  const [format, setFormat] = useState<WaFormat>(() => load());
  const [saved,  setSaved]  = useState(false);
  const [focusedLineId, setFocusedLineId] = useState<string | null>(null);

  // Active lines based on selected language
  const lang  = format.language;
  const lines = lang === "ar" ? format.ar_lines : format.en_lines;

  const setLines = useCallback((updater: (prev: WaLine[]) => WaLine[]) => {
    setFormat(f => {
      const next = updater(lang === "ar" ? f.ar_lines : f.en_lines);
      return lang === "ar" ? { ...f, ar_lines: next } : { ...f, en_lines: next };
    });
    setSaved(false);
  }, [lang]);

  // Live preview
  const preview = buildSampleMessage(lines, lang);

  // ── Handlers ──

  const handleLangSwitch = (l: WaLang) => {
    setFormat(f => ({ ...f, language: l }));
    setSaved(false);
  };

  const handleToggle = (id: string) => {
    setLines(prev => prev.map(l => l.id === id ? { ...l, enabled: !l.enabled } : l));
  };

  const handleChange = (id: string, template: string) => {
    setLines(prev => prev.map(l => l.id === id ? { ...l, template } : l));
  };

  const handleMove = (id: string, dir: -1 | 1) => {
    setLines(prev => {
      const idx = prev.findIndex(l => l.id === id);
      if (idx < 0) return prev;
      const next = [...prev];
      const target = idx + dir;
      if (target < 0 || target >= next.length) return prev;
      [next[idx], next[target]] = [next[target], next[idx]];
      return next;
    });
  };

  const handleDelete = (id: string) => {
    setLines(prev => prev.filter(l => l.id !== id));
  };

  const handleAddText = () => {
    const newLine: WaLine = {
      id: crypto.randomUUID(),
      type: "text",
      enabled: true,
      template: "",
    };
    setLines(prev => [...prev, newLine]);
    setTimeout(() => setFocusedLineId(newLine.id), 50);
  };

  const handleAddBlank = () => {
    setLines(prev => [...prev, {
      id: crypto.randomUUID(),
      type: "blank",
      enabled: true,
      template: "",
    }]);
  };

  const [resetConfirm, setResetConfirm] = useState(false);

  const handleReset = () => {
    if (!resetConfirm) {
      setResetConfirm(true);
      return;
    }
    setResetConfirm(false);
    setLines(() => lang === "ar" ? makeDefaultAr() : makeDefaultEn());
  };

  const handleSave = () => {
    save(format);
    setSaved(true);
    setTimeout(() => setSaved(false), 2500);
  };

  // Insert variable at cursor into focused text line
  const handleInsertVar = (varKey: string) => {
    setLines(prev => prev.map(l => {
      if (l.id !== focusedLineId || l.type !== "text") return l;
      return { ...l, template: l.template + `{${varKey}}` };
    }));
  };

  return (
    <div className="wame-root">

      {/* ── Language selector ── */}
      <div className="wame-lang-bar">
        <span className="wame-lang-label">Message Language:</span>
        <div className="wame-lang-btns">
          <button
            className={`wame-lang-btn${lang === "en" ? " wame-lang-active" : ""}`}
            onClick={() => handleLangSwitch("en")}
          >
            🇬🇧 English
          </button>
          <button
            className={`wame-lang-btn${lang === "ar" ? " wame-lang-active" : ""}`}
            onClick={() => handleLangSwitch("ar")}
          >
            🇸🇦 عربي
          </button>
        </div>
        <span className="wame-lang-hint">
          {lang === "en"
            ? "Message sent in English"
            : "سيتم إرسال الرسالة بالعربية"}
        </span>
      </div>

      {/* ── Main 2-panel layout ── */}
      <div className="wame-panels">

        {/* ── Left: line editor ── */}
        <div className="wame-editor-panel">
          <div className="wame-panel-header">
            <span className="wame-panel-title">Message Lines</span>
            <button className={`wame-reset-btn${resetConfirm ? " wame-reset-btn-warn" : ""}`} onClick={handleReset}>
              {resetConfirm ? "Confirm Reset?" : "↺ Reset"}
            </button>
          </div>

          <div className="wame-lines-list">
            {lines.map((line, idx) => (
              <LineRow
                key={line.id}
                line={line}
                lang={lang}
                index={idx}
                total={lines.length}
                focusedLineId={focusedLineId}
                onFocus={setFocusedLineId}
                onChange={handleChange}
                onToggle={handleToggle}
                onMove={handleMove}
                onDelete={handleDelete}
              />
            ))}
          </div>

          {/* Add buttons */}
          <div className="wame-add-row">
            <button className="wame-add-btn" onClick={handleAddText}>
              + Text Line
            </button>
            <button className="wame-add-btn" onClick={handleAddBlank}>
              + Blank Line
            </button>
          </div>

          {/* Variable reference */}
          <div className="wame-vars-section">
            <div className="wame-vars-title">
              {lang === "en" ? "Variables (click to insert into selected line):" : "المتغيرات (انقر للإدراج في السطر المحدد):"}
            </div>
            <div className="wame-vars-chips">
              {WA_VARIABLES.map(v => (
                <button
                  key={v.key}
                  className="wame-var-chip"
                  onClick={() => handleInsertVar(v.key)}
                  title={lang === "en" ? v.label_en : v.label_ar}
                >
                  {`{${v.key}}`}
                  <span className="wame-var-chip-label">
                    {lang === "en" ? v.label_en : v.label_ar}
                  </span>
                </button>
              ))}
            </div>
          </div>

          {/* Save */}
          <div className="wame-save-row">
            <button className="btn-primary wame-save-btn" onClick={handleSave}>
              {saved ? "✓ Saved" : "Save Format"}
            </button>
            {saved && <span className="wame-saved-hint">Format saved to this device.</span>}
          </div>
        </div>

        {/* ── Right: live preview ── */}
        <div className="wame-preview-panel">
          <div className="wame-panel-header">
            <span className="wame-panel-title">
              {lang === "en" ? "Preview (sample data)" : "معاينة (بيانات تجريبية)"}
            </span>
          </div>
          <div className="wame-preview-phone">
            <div className="wame-preview-bar">
              <span className="wame-preview-contact">Ahmed Al-Khalifa</span>
              <span className="wame-preview-time">14:32</span>
            </div>
            <div className="wame-preview-bubble-wrap">
              <div className={`wame-preview-bubble${lang === "ar" ? " wame-rtl" : ""}`}>
                <pre className="wame-preview-text">{preview}</pre>
              </div>
            </div>
          </div>
        </div>

      </div>
    </div>
  );
}
