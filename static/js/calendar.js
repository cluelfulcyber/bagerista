/*
 * Багериста: availability calendar clicks (static/js/calendar.js)
 *
 * The month grid is rendered by the server (partials/calendar_month.html) and
 * the two date inputs `ot` / `do` are the real state of the form. This script
 * only lets the visitor click days: first click = first day, second click =
 * last day. A period that crosses a booked (hatched) day is refused.
 * After a change it fires a normal `change` event on the form, so the price
 * calculator (calculator.js) can ask the server for a new quote.
 */
(function () {
  "use strict";
  var MONTHS = ["януари", "февруари", "март", "април", "май", "юни", "юли", "август",
    "септември", "октомври", "ноември", "декември"];

  function label(iso) {
    var p = iso.split("-");
    return parseInt(p[2], 10) + " " + MONTHS[parseInt(p[1], 10) - 1];
  }

  function init(cal) {
    var form = cal.closest("form");
    var from = form && form.querySelector("input[name=ot]");
    var to = form && form.querySelector("input[name=do]");
    if (!from || !to) return;
    var start = from.value, end = to.value;

    function days() { return Array.prototype.slice.call(cal.querySelectorAll(".cal__day[data-date]")); }
    function msg() { return cal.querySelector(".cal__msg"); }

    function paint() {
      days().forEach(function (b) {
        var t = b.dataset.date, e = end || start;
        b.classList.remove("cal__day--start", "cal__day--end", "cal__day--range", "cal__day--conflict");
        if (b.disabled) return;
        b.setAttribute("aria-pressed", String(!!start && t >= start && t <= e));
        if (!start) return;
        if (t === start) b.classList.add("cal__day--start");
        if (t === e) b.classList.add("cal__day--end");
        if (t > start && t < e) b.classList.add("cal__day--range");
      });
    }

    function bookedBetween(a, b) {
      return days().filter(function (x) {
        return x.dataset.date >= a && x.dataset.date <= b && x.classList.contains("cal__day--booked");
      });
    }

    cal.addEventListener("click", function (ev) {
      var b = ev.target.closest(".cal__day[data-date]");
      if (!b || b.disabled) return;
      var t = b.dataset.date, m = msg();
      if (!start || end) { start = t; end = ""; }
      else if (t < start) { start = t; }
      else {
        var clash = bookedBetween(start, t);
        if (clash.length) {
          clash.forEach(function (x) { x.classList.add("cal__day--conflict"); });
          m.textContent = "На " + label(clash[0].dataset.date) + " машината е заета. Изберете период без защриховани дни.";
          m.classList.add("cal__msg--error");
          return;
        }
        end = t;
      }
      m.classList.remove("cal__msg--error");
      m.textContent = end ? "Избрахте " + label(start) + " – " + label(end) + "." : "Изберете последния ден.";
      from.value = start;
      to.value = end || start;
      paint();
      form.dispatchEvent(new Event("change", { bubbles: true }));
    });

    // Typing a date keeps the calendar in sync; a month switched by htmx is repainted.
    form.addEventListener("change", function (ev) {
      if (ev.target === from || ev.target === to) { start = from.value; end = to.value; paint(); }
    });
    cal.addEventListener("htmx:afterSwap", paint);
    paint();
  }

  document.querySelectorAll("[data-calendar]").forEach(init);
})();
