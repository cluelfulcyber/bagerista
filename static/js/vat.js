/*
 * Багериста: „без ДДС / с ДДС“ switch (static/js/vat.js)
 *
 * Every switchable price is rendered by the server as
 *   <data value="16000" data-net>160,00 €</data>   (value = net price in cents)
 * and every „без ДДС“ text that belongs to such prices as <span data-vat-label>.
 * The switch is hidden in the HTML because it cannot work without JavaScript.
 * The price slip and the quote always show net, VAT and total; they are not switched.
 */
(function () {
  "use strict";
  var VAT_PERCENT = 20; // same as services::pricing::VAT_PERCENT

  // 123000 cents -> "1 230,00 €" (no-break spaces, comma decimals, like format::money)
  function money(cents) {
    var euros = Math.floor(cents / 100).toString().replace(/\B(?=(\d{3})+(?!\d))/g, " ");
    return euros + "," + String(cents % 100).padStart(2, "0") + " €";
  }
  // half-up rounding, like services::pricing::percent_of
  function percentOf(x, p) { return Math.floor((x * p + 50) / 100); }

  var vat = document.querySelector("[data-vat-switch]");
  if (!vat) return;

  function apply(gross) {
    document.querySelectorAll("[data-net]").forEach(function (el) {
      var net = parseInt(el.getAttribute("value"), 10);
      el.textContent = money(gross ? net + percentOf(net, VAT_PERCENT) : net);
    });
    document.querySelectorAll("[data-vat-label]").forEach(function (el) {
      el.textContent = gross ? "с ДДС" : "без ДДС";
    });
  }

  vat.hidden = false;
  var saved = null;
  try { saved = localStorage.getItem("bagerista-vat"); } catch (e) { /* storage blocked */ }
  if (saved === "gross") {
    vat.querySelector("[value=gross]").checked = true;
    apply(true);
  }
  vat.addEventListener("change", function (e) {
    var gross = e.target.value === "gross";
    apply(gross);
    try { localStorage.setItem("bagerista-vat", gross ? "gross" : "net"); } catch (err) { /* ignore */ }
  });
  // Prices that arrive later through htmx (catalogue filters) follow the switch too.
  document.body.addEventListener("htmx:afterSwap", function () {
    if (vat.querySelector("[value=gross]").checked) apply(true);
  });
})();
