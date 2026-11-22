import { useEffect, useId, useRef, useState } from "react";
import { Info } from "lucide-react";

export function HelpTip({ topic, children }: { topic: string; children: string }) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    const close = () => setOpen(false);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    return () => { window.removeEventListener("scroll", close, true); window.removeEventListener("resize", close); document.removeEventListener("pointerdown", outside); document.removeEventListener("keydown", escape); };
  }, [open]);
  const rect = open ? wrap.current?.getBoundingClientRect() : undefined;
  const position = rect ? {
    left: Math.max(8, Math.min(rect.left, window.innerWidth - 288)),
    ...(rect.bottom + 102 < window.innerHeight ? { top: rect.bottom } : { bottom: window.innerHeight - rect.top }),
  } : undefined;
  return <span ref={wrap} className="help-tip" onMouseEnter={() => setOpen(true)} onMouseLeave={() => setOpen(false)}>
    <button type="button" aria-label={"What is " + topic + "?"} aria-describedby={open ? id : undefined}
      onFocus={() => setOpen(true)} onBlur={() => setOpen(false)} onClick={() => setOpen(true)}><Info size={15} /></button>
    {open && <span className="help-bubble" style={position} id={id} role="tooltip">{children}</span>}
  </span>;
}
