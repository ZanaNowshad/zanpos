import { useEffect, useRef, useState } from "react";

const BOOT_LINES = [
  { delay: 0,   text: "ZANPOS v2.4.1 — Boot sequence initiated" },
  { delay: 130, text: "Loading kernel modules ............. OK" },
  { delay: 260, text: "Mounting local database ............ OK" },
  { delay: 390, text: "Checking fiscal integrity .......... OK" },
  { delay: 510, text: "Warming product cache .............. OK" },
  { delay: 630, text: "Initialising payment bridge ........ OK" },
  { delay: 740, text: "Syncing VAT rules .................. OK" },
  { delay: 850, text: "AI subsystem ready ................. OK" },
];

const TAGLINE = "AI-FIRST. BUSINESS OPERATIONS.";

type Phase = "booting" | "fading" | "logo" | "divider" | "typing" | "done";

export default function ZanposBootLogo() {
  // How many boot log lines are visible
  const [visibleLines, setVisibleLines] = useState(0);
  const [phase, setPhase]               = useState<Phase>("booting");
  const [typed, setTyped]               = useState("");
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);

  const after = (fn: () => void, ms: number) => {
    const t = setTimeout(fn, ms);
    timers.current.push(t);
  };

  useEffect(() => {
    const activeTimers = timers.current;
    // Reveal lines one-by-one
    BOOT_LINES.forEach((line, i) => {
      after(() => setVisibleLines(i + 1), line.delay + 80);
    });

    after(() => setPhase("fading"), 1380);   // overlay fades out
    after(() => setPhase("logo"),   1700);   // logo slides in
    after(() => setPhase("divider"),2100);   // divider appears
    after(() => setPhase("typing"), 2500);   // typewriter starts

    return () => { activeTimers.forEach(clearTimeout); };
  }, []);

  // Typewriter
  useEffect(() => {
    if (phase !== "typing") return;
    let i = 0;
    const iv = setInterval(() => {
      i++;
      setTyped(TAGLINE.slice(0, i));
      if (i >= TAGLINE.length) { clearInterval(iv); setPhase("done"); }
    }, 38);
    return () => clearInterval(iv);
  }, [phase]);

  const overlayActive  = phase === "booting" || phase === "fading";
  const logoVisible    = phase !== "booting" && phase !== "fading";
  const dividerVisible = phase === "divider" || phase === "typing" || phase === "done";
  const footerVisible  = logoVisible;

  return (
    <>
      {/* Terminal boot overlay */}
      <div
        className={[
          "zanpos-boot-overlay",
          phase === "fading" ? "zanpos-boot-overlay--fading" : "",
        ].join(" ")}
        aria-hidden={!overlayActive}
        style={!overlayActive ? { display: "none" } : undefined}
      >
        <div className="zanpos-boot-log">
          {BOOT_LINES.map((line, i) => (
            <div
              key={i}
              className={`zanpos-boot-line${i < visibleLines ? " zanpos-boot-line--visible" : ""}`}
            >
              {line.text}
            </div>
          ))}
        </div>
        <div className="zanpos-boot-status">ZANPOS • STARTING</div>
      </div>

      {/* Scanline — always present */}
      <div className="zanpos-boot-scanline" />

      {/* Brand content */}
      <div className="zanpos-boot-content">
        <div className="zanpos-boot-logo-wrap">
          <span className={`zanpos-boot-zan${logoVisible ? " zanpos-boot-zan--visible" : ""}`}>
            ZAN
          </span>
          <span className={`zanpos-boot-pos${logoVisible ? " zanpos-boot-pos--visible" : ""}`}>
            POS
          </span>
        </div>

        <div className={`zanpos-boot-divider${dividerVisible ? " zanpos-boot-divider--visible" : ""}`} />

        <div className="zanpos-boot-tagline">
          {phase === "typing" || phase === "done" ? (
            <>
              {typed}
              {phase === "typing" && <span className="zanpos-boot-cursor">|</span>}
            </>
          ) : (
            /* Keep height reserved so layout doesn't jump when text appears */
            <span style={{ visibility: "hidden" }}>{TAGLINE}</span>
          )}
        </div>
      </div>

      {/* Footer badge */}
      <div className={`zanpos-boot-footer${footerVisible ? " zanpos-boot-footer--visible" : ""}`}>
        <span>⚡</span>
        <span>Secure · Reliable · Local</span>
      </div>
    </>
  );
}
