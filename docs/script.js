const tabButtons = Array.from(document.querySelectorAll("[data-tab]"));
const tabPanels = Array.from(document.querySelectorAll("[data-panel]"));
const copyButtons = Array.from(document.querySelectorAll("[data-copy-target]"));
const revealNodes = Array.from(document.querySelectorAll("[data-reveal]"));

function setActiveTab(name) {
  tabButtons.forEach((button) => {
    const isActive = button.dataset.tab === name;
    button.classList.toggle("is-active", isActive);
    button.setAttribute("aria-selected", String(isActive));
    button.tabIndex = isActive ? 0 : -1;
  });

  tabPanels.forEach((panel) => {
    const isActive = panel.dataset.panel === name;
    panel.classList.toggle("is-active", isActive);
    panel.hidden = !isActive;
  });
}

tabButtons.forEach((button) => {
  button.addEventListener("click", () => setActiveTab(button.dataset.tab));
});

copyButtons.forEach((button) => {
  button.addEventListener("click", async () => {
    const target = document.getElementById(button.dataset.copyTarget);

    if (!target) {
      return;
    }

    const originalLabel = button.textContent;

    try {
      await navigator.clipboard.writeText(target.textContent.trim());
      button.textContent = "Copied";
      button.classList.add("is-copied");
    } catch {
      button.textContent = "Select manually";
    }

    window.setTimeout(() => {
      button.textContent = originalLabel;
      button.classList.remove("is-copied");
    }, 1800);
  });
});

if ("IntersectionObserver" in window) {
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          entry.target.classList.add("is-visible");
          observer.unobserve(entry.target);
        }
      });
    },
    {
      threshold: 0.18,
      rootMargin: "0px 0px -40px 0px",
    }
  );

  revealNodes.forEach((node) => observer.observe(node));
} else {
  revealNodes.forEach((node) => node.classList.add("is-visible"));
}

const yearNode = document.getElementById("footer-year");

if (yearNode) {
  yearNode.textContent = new Date().getFullYear();
}
