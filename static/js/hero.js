/*
 * Багериста: hero drawing (static/js/hero.js)
 *
 * The drawing is a side view of a Kubota KX057-6 at 1 unit = 10 mm, with the
 * ground at y = 0. The server renders the arm already at maximum depth, so
 * the page is complete without JavaScript. This script adds:
 *   1. one load sequence: the body is plotted like a pen plotter, then the
 *      arm lowers from the parked pose into the dash-dot phantom;
 *   2. a slider that moves the arm; a live dimension shows the depth in mm.
 *
 * Kinematics: three absolute angles (boom b, stick s, bucket k). Each part is
 * drawn in its own frame with its pin at (0, 0) pointing along +x, and placed
 * with translate(pin) rotate(angle). The hydraulic cylinders are recomputed
 * so they stretch between their two pins (forward kinematics, ~40 lines).
 */
(function () {
  "use strict";

  var P = {
    pivot: [360, -95], base: [372, -60], L1: 300, L2: 180,
    tooth: [92, -4], boomLug: [128, -8], stickCylBoom: [150, -46],
    stickLever: [-38, -16], bucketCylStick: [24, -22], bucketEar: [6, -26],
    barrels: [78, 112, 124]
  };
  var REST = { b: -34, s: 66, k: 140 };   // parked: arm raised, bucket curled
  var DIG = { b: 44, s: 100, k: 77.75 };  // teeth 380 units = 3 800 mm below ground

  function rot(p, deg) {
    var r = deg * Math.PI / 180, c = Math.cos(r), s = Math.sin(r);
    return [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
  }
  function add(a, b) { return [a[0] + b[0], a[1] + b[1]]; }
  function lerp(a, b, t) { return { b: a.b + (b.b - a.b) * t, s: a.s + (b.s - a.s) * t, k: a.k + (b.k - a.k) * t }; }

  function pose(a) {
    var bt = add(P.pivot, rot([P.L1, 0], a.b));
    var st = add(bt, rot([P.L2, 0], a.s));
    return {
      bt: bt, st: st, tooth: add(st, rot(P.tooth, a.k)),
      lug: add(P.pivot, rot(P.boomLug, a.b)),
      sA: add(P.pivot, rot(P.stickCylBoom, a.b)), sB: add(bt, rot(P.stickLever, a.s)),
      kA: add(bt, rot(P.bucketCylStick, a.s)), kB: add(st, rot(P.bucketEar, a.k))
    };
  }

  function n(v) { return v.toFixed(1); }
  function place(el, at, deg) { el.setAttribute("transform", "translate(" + n(at[0]) + " " + n(at[1]) + ") rotate(" + n(deg) + ")"); }

  // A cylinder group holds: rod line (A→B), two barrel lines (A→E), two pin circles.
  function cylinder(g, A, B, barrel) {
    var dx = B[0] - A[0], dy = B[1] - A[1], len = Math.hypot(dx, dy) || 1;
    var L = Math.min(barrel, len - 6), E = [A[0] + dx / len * L, A[1] + dy / len * L];
    g.querySelectorAll("line").forEach(function (ln) {
      var end = ln.classList.contains("rod") ? B : E;
      ln.setAttribute("x1", n(A[0])); ln.setAttribute("y1", n(A[1]));
      ln.setAttribute("x2", n(end[0])); ln.setAttribute("y2", n(end[1]));
    });
    var c = g.querySelectorAll("circle");
    c[0].setAttribute("cx", n(A[0])); c[0].setAttribute("cy", n(A[1]));
    c[1].setAttribute("cx", n(B[0])); c[1].setAttribute("cy", n(B[1]));
  }

  function apply(arm, a) {
    var p = pose(a);
    place(arm.querySelector("[data-part=boom]"), P.pivot, a.b);
    place(arm.querySelector("[data-part=stick]"), p.bt, a.s);
    place(arm.querySelector("[data-part=bucket]"), p.st, a.k);
    cylinder(arm.querySelector("[data-part=cyl-boom]"), P.base, p.lug, P.barrels[0]);
    cylinder(arm.querySelector("[data-part=cyl-stick]"), p.sA, p.sB, P.barrels[1]);
    cylinder(arm.querySelector("[data-part=cyl-bucket]"), p.kA, p.kB, P.barrels[2]);
    return p;
  }

  // Bulgarian number format: space between thousands.
  function mm(v) { return String(v).replace(/\B(?=(\d{3})+(?!\d))/g, " "); }

  // Same curve as the CSS token --ease-hydraulic: slow start, firm stop.
  function hydraulic(t) { return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2; }

  function init(fig) {
    var svg = fig.querySelector("svg");
    var arm = svg.querySelector("[data-arm]");
    var live = svg.querySelector("[data-live]");
    var lLine = svg.querySelector("[data-live-line]"), lExt = svg.querySelector("[data-live-ext]");
    var lBg = svg.querySelector("[data-live-bg]"), lText = svg.querySelector("[data-live-text]");
    var control = document.getElementById(fig.dataset.control);
    var slider = control.querySelector("input[type=range]");
    var out = control.querySelector("output");

    function render(t) {
      var p = apply(arm, lerp(REST, DIG, t));
      var depth = Math.max(0, Math.round(p.tooth[1]) * 10);     // 1 unit = 10 mm
      var y = Math.max(2, p.tooth[1]);
      live.style.opacity = depth > 60 ? 1 : 0;
      lLine.setAttribute("d", "M716 2 V" + n(y - 2));
      lExt.setAttribute("d", "M" + n(p.tooth[0] + 10) + " " + n(y) + " H724");
      lBg.setAttribute("y", n(y / 2 - 17));
      lText.setAttribute("y", n(y / 2 + 5));
      lText.textContent = mm(depth) + " мм";
      out.textContent = depth > 0 ? mm(depth) + " мм" : "над терена";
      slider.setAttribute("aria-valuetext", depth > 0 ? "дълбочина " + mm(depth) + " милиметра" : "кофата е над терена");
    }

    control.hidden = false;
    slider.addEventListener("input", function () { render(slider.value / 100); });

    var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce) { slider.value = 100; render(1); return; }

    // The single orchestrated moment: plot the body, then dig into the phantom.
    var plotted = svg.querySelectorAll(".exc-body path, .exc-body rect, .exc-body circle, [data-arm] .paint, [data-arm] .iron");
    plotted.forEach(function (el) { el.setAttribute("pathLength", "1"); el.classList.add("plot"); });
    svg.querySelectorAll(".dim, .numeral, .numeral-label").forEach(function (el) { el.classList.add("plot-late"); });
    fig.classList.add("is-plotting");
    slider.value = 0; render(0);

    setTimeout(function () {
      fig.classList.remove("is-plotting");
      plotted.forEach(function (el) { el.removeAttribute("pathLength"); el.classList.remove("plot"); });
      var start = null, dur = 1400;
      function step(ts) {
        if (start === null) start = ts;
        var t = Math.min(1, (ts - start) / dur);
        var e = hydraulic(t);
        slider.value = Math.round(e * 100);
        render(e);
        if (t < 1) requestAnimationFrame(step);
      }
      requestAnimationFrame(step);
    }, 1300);
  }

  document.querySelectorAll("[data-hero-drawing]").forEach(init);
})();
