/*
 * Багериста: mobile navigation and dismissible messages (static/js/nav.js)
 * Below 1024 px the main menu hides behind the „Меню“ button. Without
 * JavaScript the button does nothing and the footer links still work.
 * Flash messages get their × button only when this script runs.
 */
(function () {
  "use strict";
  document.querySelectorAll(".flash__close").forEach(function (button) {
    button.hidden = false;
    button.addEventListener("click", function () { button.closest(".flash").remove(); });
  });

  var toggle = document.querySelector(".nav-toggle");
  var nav = document.getElementById("main-nav");
  if (!toggle || !nav) return;
  toggle.addEventListener("click", function () {
    var open = toggle.getAttribute("aria-expanded") === "true";
    toggle.setAttribute("aria-expanded", String(!open));
    nav.classList.toggle("is-open", !open);
  });
})();
