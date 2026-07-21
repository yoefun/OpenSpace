/** Subtle particle field behind the UI — pointer-events none, low motion. */

export function mountBackground(root: HTMLElement): () => void {
  root.innerHTML = `
    <div class="orb orb-a" aria-hidden="true"></div>
    <div class="orb orb-b" aria-hidden="true"></div>
    <div class="orb orb-c" aria-hidden="true"></div>
    <div class="grid" aria-hidden="true"></div>
    <div class="scan" aria-hidden="true"></div>
    <canvas aria-hidden="true"></canvas>
  `;

  const canvas = root.querySelector("canvas")!;
  const ctx = canvas.getContext("2d");
  if (!ctx) return () => undefined;

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  let raf = 0;
  let w = 0;
  let h = 0;

  type Dot = { x: number; y: number; vx: number; vy: number; r: number; a: number };
  let dots: Dot[] = [];

  const resize = () => {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    w = window.innerWidth;
    h = window.innerHeight;
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const count = Math.min(64, Math.floor((w * h) / 28000));
    dots = Array.from({ length: count }, () => ({
      x: Math.random() * w,
      y: Math.random() * h,
      vx: (Math.random() - 0.5) * 0.18,
      vy: (Math.random() - 0.5) * 0.18,
      r: 0.6 + Math.random() * 1.4,
      a: 0.15 + Math.random() * 0.35,
    }));
  };

  const draw = () => {
    ctx.clearRect(0, 0, w, h);
    for (const d of dots) {
      if (!reduceMotion) {
        d.x += d.vx;
        d.y += d.vy;
        if (d.x < -10) d.x = w + 10;
        if (d.x > w + 10) d.x = -10;
        if (d.y < -10) d.y = h + 10;
        if (d.y > h + 10) d.y = -10;
      }
      ctx.beginPath();
      ctx.fillStyle = `rgba(46, 230, 255, ${d.a})`;
      ctx.arc(d.x, d.y, d.r, 0, Math.PI * 2);
      ctx.fill();
    }

    // Soft links between nearby dots — keeps HUD feel without clutter
    ctx.strokeStyle = "rgba(46, 230, 255, 0.05)";
    ctx.lineWidth = 1;
    for (let i = 0; i < dots.length; i++) {
      for (let j = i + 1; j < dots.length; j++) {
        const a = dots[i]!;
        const b = dots[j]!;
        const dx = a.x - b.x;
        const dy = a.y - b.y;
        const dist = Math.hypot(dx, dy);
        if (dist < 110) {
          ctx.globalAlpha = 1 - dist / 110;
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }
    }
    ctx.globalAlpha = 1;

    if (!reduceMotion) raf = requestAnimationFrame(draw);
  };

  const onResize = () => {
    resize();
    if (reduceMotion) draw();
  };

  resize();
  draw();
  window.addEventListener("resize", onResize);

  return () => {
    cancelAnimationFrame(raf);
    window.removeEventListener("resize", onResize);
  };
}
