import { useEffect, useId, useState } from "react";
import "./bird.css";

/** Dauer der Putz-Animation, wie `.bird.preen` in bird.css. */
const PREEN_MS = 1600;

/** Alle paar Sekunden putzt sich der Vogel, in unregelmäßigen Abständen. */
function usePreening(enabled: boolean) {
  const [preening, setPreening] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let timer: number;
    const next = () => {
      timer = window.setTimeout(() => {
        setPreening(true);
        timer = window.setTimeout(() => {
          setPreening(false);
          next();
        }, PREEN_MS);
      }, 6000 + Math.random() * 8000);
    };
    next();
    return () => {
      clearTimeout(timer);
      setPreening(false);
    };
  }, [enabled]);
  return preening;
}

/** Der Birdy-Eisvogel – dieselbe Grafik wie im Tray von Birdy Credit Status
 *  (Quelle: src/assets/birdy-kingfisher.svg). Der Schnabel ist hier in Ober-
 *  und Unterschnabel geteilt: `open` (0–1) öffnet ihn, so singt der Vogel in
 *  der Pille zur Stimme mit. Geschlossen ergibt sich exakt die Originalform.
 *  Die Klassen `upper` und `lower` lassen CSS-Animationen den Schnabel bewegen;
 *  mit `preen` putzt er sich ab und zu. */
export function Bird({
  size = 24,
  open = 0,
  preen = false,
  className = "",
}: {
  size?: number;
  open?: number;
  preen?: boolean;
  className?: string;
}) {
  // Eindeutige Verlaufs-IDs, falls mehrere Vögel im selben Dokument stehen.
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  const ref = (name: string) => `url(#${id}${name})`;
  const angle = Math.max(0, Math.min(1, open)) * 16;
  const preening = usePreening(preen);
  return (
    <svg className={`bird ${preening ? "preen" : ""} ${className}`} width={size} height={size} viewBox="0 0 256 256" aria-hidden="true">
      <defs>
        <linearGradient id={`${id}crown`} x1="96" y1="30" x2="173" y2="166" gradientUnits="userSpaceOnUse">
          <stop stopColor="#47CEC9" />
          <stop offset="0.48" stopColor="#189BB4" />
          <stop offset="1" stopColor="#13618E" />
        </linearGradient>
        <linearGradient id={`${id}breast`} x1="141" y1="102" x2="157" y2="210" gradientUnits="userSpaceOnUse">
          <stop stopColor="#FBC782" />
          <stop offset="0.5" stopColor="#EEA15B" />
          <stop offset="1" stopColor="#D57538" />
        </linearGradient>
        <linearGradient id={`${id}wing`} x1="79" y1="105" x2="105" y2="194" gradientUnits="userSpaceOnUse">
          <stop stopColor="#288FB4" />
          <stop offset="1" stopColor="#164968" />
        </linearGradient>
        <linearGradient id={`${id}bill`} x1="196" y1="62" x2="196" y2="83" gradientUnits="userSpaceOnUse">
          <stop stopColor="#88A5AE" />
          <stop offset="0.48" stopColor="#4E7285" />
          <stop offset="0.52" stopColor="#2D4B61" />
        </linearGradient>
      </defs>
      <path fill="#245D78" d="M62 169L97 195L36 225L19 220Z" />
      <path
        d="M98 201L93 224L105 228M93 224L86 227M123 201L118 224L131 227M118 224L111 228"
        fill="none"
        stroke="#AA6346"
        strokeWidth="4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      {/* Schnabel, Gelenk im Mundwinkel (173|72); die Basis liegt unter dem Kopf. */}
      <path className="mandible upper" fill={ref("bill")} d="M173 62L246 74L173 72Z" style={{ transform: `rotate(${-angle}deg)`, transformOrigin: "173px 72px" }} />
      <path className="mandible lower" fill={ref("bill")} d="M173 72L246 74L173 82Z" style={{ transform: `rotate(${angle}deg)`, transformOrigin: "173px 72px" }} />
      <path
        fill={ref("crown")}
        d="M34 197C43 158 63 114 87 86C85 70 89 50 104 38C120 27 143 27 160 35C177 43 183 60 177 77L169 92C179 108 177 134 164 158C146 193 124 213 99 211C73 211 49 198 34 197Z"
      />
      <path fill={ref("breast")} d="M169 89C179 107 177 133 164 158C146 193 124 213 99 211C87 211 76 207 67 202C91 187 108 166 120 141C130 118 145 100 169 89Z" />
      <path fill="#F8EDCF" d="M170 88C175 97 177 111 174 122C165 119 158 111 154 102C160 97 165 92 170 88Z" />
      <path fill="#D98A48" d="M114 71C128 78 143 78 156 73L163 85C150 97 135 99 122 91Z" />
      <path fill="#F6EDD8" d="M111 78L124 92L115 102C109 99 104 93 103 89Z" />
      <path fill="#5BDFCE" d="M92 95C72 124 56 157 42 190L34 197C44 157 66 116 86 91Z" />
      <path fill={ref("wing")} d="M123 99C105 90 85 108 69 131C53 154 42 177 34 197C65 193 94 174 114 148C128 129 134 111 123 99Z" />
      <path d="M122 110C110 135 88 157 61 179M119 129C105 151 86 166 69 175" fill="none" stroke="#6EC9CD" strokeWidth="2" strokeLinecap="round" opacity="0.35" />
      <path d="M104 46C119 35 142 35 155 42" fill="none" stroke="#A0E3D8" strokeWidth="3" strokeLinecap="round" opacity="0.55" />
      <circle fill="#102936" cx="155" cy="60" r="5.5" />
      <circle fill="#F7F2DE" cx="153.5" cy="58.5" r="1.4" />
    </svg>
  );
}
