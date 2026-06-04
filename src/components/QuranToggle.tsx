import { useEffect, useRef, useState, useCallback } from "react";

/**
 * Quran audio toggle for the POS top bar.
 *
 * Plays the full Quran continuously (Surah 1 → 114, then loops) for true 24/7
 * playback. Audio comes from the free, key-less, rate-limit-free QuranAPI project
 * (https://quranapi.pages.dev), whose chapter MP3s are hosted on GitHub with a
 * uniform, predictable URL pattern:
 *
 *   https://github.com/The-Quran-Project/Quran-Audio-Chapters/raw/refs/heads/main/Data/{reciter}/{surah}.mp3
 *
 * Every file is Content-Type: audio/mpeg and plays directly in a plain <audio>
 * element. We construct each surah URL on the fly and auto-advance on `ended`,
 * so there are no per-track API calls and no streaming server to depend on.
 *
 * State (on/off, reciter, surah position, volume) persists in localStorage so the
 * cashier's preference and place survive reloads. Audio only starts on an explicit
 * user click (browsers block autoplay), so it never starts on its own.
 */

interface Reciter {
  id: number;   // QuranAPI reciter number
  name: string;
}

// QuranAPI's bundled reciters (the famous ones requested).
const RECITERS: Reciter[] = [
  { id: 1, name: "Mishary Rashid Al-Afasy" },
  { id: 2, name: "Abu Bakr Al-Shatri" },
  { id: 3, name: "Nasser Al-Qatami" },
  { id: 4, name: "Yasser Al-Dosari" },
  { id: 5, name: "Hani Ar-Rifai" },
];

const TOTAL_SURAHS = 114;
// Use github.com/raw/: its final hop serves Content-Type: audio/mpeg with
// Access-Control-Allow-Origin: * (the raw.githubusercontent.com direct form
// serves text/plain which `nosniff` would block in an <audio> element).
const BASE = "https://github.com/The-Quran-Project/Quran-Audio-Chapters/raw/refs/heads/main/Data";

const LS_ON      = "zanpos_quran_on";
const LS_RECITER = "zanpos_quran_reciter";
const LS_SURAH   = "zanpos_quran_surah";
const LS_VOLUME  = "zanpos_quran_volume";

function surahUrl(reciter: number, surah: number): string {
  return `${BASE}/${reciter}/${surah}.mp3`;
}

export default function QuranToggle() {
  const audioRef = useRef<HTMLAudioElement | null>(null);

  // Online status — the toggle only appears when an internet connection exists,
  // since the audio streams from the network (no offline files are bundled).
  const [online, setOnline] = useState<boolean>(
    typeof navigator !== "undefined" ? navigator.onLine : true
  );

  const [playing, setPlaying]   = useState(false);
  const [loading, setLoading]   = useState(false);
  const [showMenu, setShowMenu] = useState(false);

  const [reciterId, setReciterId] = useState<number>(() => {
    const v = parseInt(localStorage.getItem(LS_RECITER) ?? "1", 10);
    return RECITERS.some(r => r.id === v) ? v : 1;
  });
  // Current surah position (1-114). Resumes where the cashier left off.
  const surahRef = useRef<number>((() => {
    const v = parseInt(localStorage.getItem(LS_SURAH) ?? "1", 10);
    return v >= 1 && v <= TOTAL_SURAHS ? v : 1;
  })());
  const [surahNo, setSurahNo] = useState<number>(surahRef.current);

  const [volume, setVolume] = useState<number>(() => {
    const v = parseFloat(localStorage.getItem(LS_VOLUME) ?? "0.6");
    return isNaN(v) ? 0.6 : Math.min(1, Math.max(0, v));
  });

  const reciter = RECITERS.find(r => r.id === reciterId) ?? RECITERS[0];

  // Lazily create the audio element once.
  if (!audioRef.current && typeof Audio !== "undefined") {
    const a = new Audio();
    a.preload = "none";
    a.volume = volume;
    audioRef.current = a;
  }

  const stop = useCallback(() => {
    const a = audioRef.current;
    if (a) { a.pause(); }
    setPlaying(false);
    setLoading(false);
  }, []);

  // Track connectivity. When the connection drops, hide the toggle and stop any
  // playback (the stream can't continue offline). It reappears when back online.
  useEffect(() => {
    const goOnline  = () => setOnline(true);
    const goOffline = () => { setOnline(false); stop(); };
    window.addEventListener("online", goOnline);
    window.addEventListener("offline", goOffline);
    return () => {
      window.removeEventListener("online", goOnline);
      window.removeEventListener("offline", goOffline);
    };
  }, [stop]);

  const playSurah = useCallback((rid: number, surah: number) => {
    const a = audioRef.current;
    if (!a) return;
    surahRef.current = surah;
    setSurahNo(surah);
    setLoading(true);
    a.src = surahUrl(rid, surah);
    a.volume = volume;
    a.play()
      .then(() => { setPlaying(true); setLoading(false); })
      .catch(() => { setPlaying(false); setLoading(false); });
  }, [volume]);

  // Auto-advance: when a surah finishes, play the next (loop 114 → 1).
  // Also recover from a load error by skipping to the next surah.
  useEffect(() => {
    const a = audioRef.current;
    if (!a) return;
    const next = () => {
      const upcoming = surahRef.current >= TOTAL_SURAHS ? 1 : surahRef.current + 1;
      playSurah(reciterId, upcoming);
    };
    const onEnded   = () => next();
    const onError   = () => { if (playing || loading) next(); };
    const onPlaying = () => { setPlaying(true); setLoading(false); };
    const onWaiting = () => setLoading(true);
    a.addEventListener("ended", onEnded);
    a.addEventListener("error", onError);
    a.addEventListener("playing", onPlaying);
    a.addEventListener("waiting", onWaiting);
    return () => {
      a.removeEventListener("ended", onEnded);
      a.removeEventListener("error", onError);
      a.removeEventListener("playing", onPlaying);
      a.removeEventListener("waiting", onWaiting);
    };
  }, [reciterId, playing, loading, playSurah]);

  // Persist + apply volume.
  useEffect(() => {
    if (audioRef.current) audioRef.current.volume = volume;
    localStorage.setItem(LS_VOLUME, String(volume));
  }, [volume]);

  // Persist on/off, reciter, surah position.
  useEffect(() => { localStorage.setItem(LS_ON, playing ? "1" : "0"); }, [playing]);
  useEffect(() => { localStorage.setItem(LS_RECITER, String(reciterId)); }, [reciterId]);
  useEffect(() => { localStorage.setItem(LS_SURAH, String(surahNo)); }, [surahNo]);

  // Clean up on unmount.
  useEffect(() => () => { const a = audioRef.current; if (a) { a.pause(); a.src = ""; } }, []);

  const toggle = () => {
    if (playing || loading) stop();
    else playSurah(reciterId, surahRef.current);
  };

  const pickReciter = (r: Reciter) => {
    setReciterId(r.id);
    setShowMenu(false);
    if (playing || loading) playSurah(r.id, surahRef.current);
  };

  const icon = loading ? "◌" : playing ? "🔊" : "🔇";

  // Hide entirely when offline — the audio is network-streamed, never bundled.
  if (!online) return null;

  return (
    <div className="quran-toggle-wrap">
      <button
        className={`top-bar-btn quran-toggle-btn${playing ? " quran-toggle-on" : ""}`}
        onClick={toggle}
        onContextMenu={(e) => { e.preventDefault(); setShowMenu(m => !m); }}
        title={playing
          ? `Playing: ${reciter.name} — Surah ${surahNo}/114 (click to stop, right-click for reciter)`
          : "Play Quran — right-click to choose reciter"}
        data-tauri-drag-region="false"
      >
        <span className={`quran-icon${loading ? " quran-icon-spin" : ""}`}>{icon}</span>
        <span className="quran-label">Quran</span>
        <span className="quran-caret" onClick={(e) => { e.stopPropagation(); setShowMenu(m => !m); }}>▾</span>
      </button>

      {showMenu && (
        <>
          <div className="quran-menu-backdrop" onClick={() => setShowMenu(false)} />
          <div className="quran-menu" role="menu">
            <div className="quran-menu-title">Reciter</div>
            {RECITERS.map(r => (
              <button
                key={r.id}
                className={`quran-menu-item${r.id === reciterId ? " quran-menu-item-active" : ""}`}
                onClick={() => pickReciter(r)}
                role="menuitemradio"
                aria-checked={r.id === reciterId}
              >
                {r.id === reciterId && <span className="quran-menu-check">✓</span>}
                <span className="quran-menu-name">{r.name}</span>
              </button>
            ))}

            {playing && (
              <div className="quran-menu-now">Now: Surah {surahNo} / {TOTAL_SURAHS}</div>
            )}

            <div className="quran-menu-vol">
              <span className="quran-menu-vol-icon">🔈</span>
              <input
                type="range"
                min={0} max={1} step={0.05}
                value={volume}
                onChange={(e) => setVolume(parseFloat(e.target.value))}
                aria-label="Quran volume"
              />
              <span className="quran-menu-vol-icon">🔊</span>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
