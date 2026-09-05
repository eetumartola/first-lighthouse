// Trunk initializer: reports load progress and requires WebGPU (the volumetric fog and the sea
// material need it; WebGL2 is not a target).
//
// Known issue: browsers keep the engine's AudioContext suspended until a user gesture, and the
// engine creates it at startup, so web audio may stay silent. Not addressed yet.
const status = () => document.getElementById("status");
const show = (html) => { const s = status(); if (s) { s.innerHTML = html; s.classList.remove("hidden"); } };

export default function init() {
  if (!("gpu" in navigator)) {
    show("First Lighthouse needs WebGPU.<br /><small>Use a current Chrome, Edge, Firefox or Safari.</small>");
    return {
      onStart: () => {},
      onProgress: () => {},
      onComplete: () => {},
      onSuccess: () => {},
      onFailure: () => {},
    };
  }
  return {
    onStart: () => show("First Lighthouse<br /><small>downloading…</small>"),
    onProgress: ({ current, total }) => {
      if (total) show(`First Lighthouse<br /><small>downloading… ${Math.round((100 * current) / total)}%</small>`);
    },
    onComplete: () => show("First Lighthouse<br /><small>starting…</small>"),
    onSuccess: () => {
      // The app draws its own menu; drop the overlay once the first frames have had time to render.
      setTimeout(() => status()?.classList.add("hidden"), 1500);
    },
    onFailure: (error) => show(`Failed to start.<br /><small>${String(error)}</small>`),
  };
}
