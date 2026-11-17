/*
 * Багериста: wizard teaser on the home page (static/js/wizard.js)
 *
 * Choosing a job moves the dashed „нужни … м“ line on the soil chart and
 * shows the matching answer (each radio tile carries it in a <template>).
 * The server already renders the default job, so the panel is never empty.
 */
(function () {
  "use strict";
  var chart = document.querySelector("[data-soilchart]");
  var answer = document.querySelector("[data-wizard-answer]");
  if (!chart || !answer) return;
  var needLabel = chart.querySelector(".soilchart__need-label");

  document.querySelectorAll("input[name=rabota][data-need]").forEach(function (radio) {
    radio.addEventListener("change", function () {
      var need = parseFloat(radio.dataset.need);
      chart.style.setProperty("--need", need);
      needLabel.textContent = "нужни " + String(need).replace(".", ",") + " м";
      chart.querySelectorAll(".soilchart__col").forEach(function (col) {
        col.classList.toggle("is-short", parseFloat(col.dataset.depth) < need);
      });
      var tpl = radio.parentElement.querySelector("template");
      if (tpl) answer.replaceChildren(tpl.content.cloneNode(true));
    });
  });
})();
