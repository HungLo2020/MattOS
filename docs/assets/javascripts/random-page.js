(function () {
  const SCRIPT_PATH = "assets/javascripts/random-page.js";
  const BUTTON_ID = "mattos-random-page";
  const STORAGE_KEY = "mattos:last-random-page";

  function getScriptUrl() {
    if (document.currentScript && document.currentScript.src) {
      return new URL(document.currentScript.src, window.location.href);
    }

    const scripts = Array.from(document.scripts);
    const script = scripts.find((candidate) => candidate.src && candidate.src.includes(SCRIPT_PATH));
    return script ? new URL(script.src, window.location.href) : new URL(window.location.href);
  }

  function getSiteBase() {
    const scriptUrl = getScriptUrl();
    const pathname = scriptUrl.pathname;
    const index = pathname.indexOf(SCRIPT_PATH);

    if (index >= 0) {
      scriptUrl.pathname = pathname.slice(0, index);
      scriptUrl.search = "";
      scriptUrl.hash = "";
      return scriptUrl;
    }

    return new URL("./", document.baseURI);
  }

  const siteBase = getSiteBase();
  const searchIndexUrl = new URL("search/search_index.json", siteBase);
  let pagesPromise = null;

  function canonicalPath(url) {
    let pathname = url.pathname.replace(/\/index\.html$/, "/");
    if (pathname.length > 1) {
      pathname = pathname.replace(/\/$/, "");
    }
    return pathname;
  }

  function pageUrlFromLocation(location) {
    const rawLocation = String(location || "").trim();
    const pageLocation = rawLocation.split("#")[0].trim();
    return new URL(pageLocation || ".", siteBase);
  }

  async function loadPages() {
    if (!pagesPromise) {
      pagesPromise = fetch(searchIndexUrl, { cache: "no-store" })
        .then((response) => {
          if (!response.ok) {
            throw new Error(`Unable to load wiki search index: ${response.status}`);
          }
          return response.json();
        })
        .then((index) => {
          const pages = new Map();
          for (const entry of index.docs || []) {
            const url = pageUrlFromLocation(entry.location);
            if (!url) {
              continue;
            }

            const key = canonicalPath(url);
            if (!pages.has(key)) {
              pages.set(key, {
                title: entry.title || "Random page",
                url
              });
            }
          }
          return Array.from(pages.values());
        });
    }

    return pagesPromise;
  }

  function pickRandomPage(pages) {
    const currentPath = canonicalPath(new URL(window.location.href));
    const lastPath = sessionStorage.getItem(STORAGE_KEY);
    let candidates = pages.filter((page) => {
      const pagePath = canonicalPath(page.url);
      return pagePath !== currentPath && pagePath !== lastPath;
    });

    if (candidates.length === 0) {
      candidates = pages.filter((page) => canonicalPath(page.url) !== currentPath);
    }
    if (candidates.length === 0) {
      candidates = pages;
    }

    return candidates[Math.floor(Math.random() * candidates.length)];
  }

  async function goToRandomPage(button) {
    button.disabled = true;
    try {
      const pages = await loadPages();
      if (pages.length === 0) {
        throw new Error("No wiki pages found in the search index.");
      }

      const page = pickRandomPage(pages);
      sessionStorage.setItem(STORAGE_KEY, canonicalPath(page.url));
      window.location.assign(page.url.href);
    } catch (error) {
      console.error(error);
      button.disabled = false;
    }
  }

  function addStyles() {
    if (document.getElementById(`${BUTTON_ID}-styles`)) {
      return;
    }

    const style = document.createElement("style");
    style.id = `${BUTTON_ID}-styles`;
    style.textContent = `
      .mattos-random-page {
        align-items: center;
        background: color-mix(in srgb, var(--md-primary-fg-color--light) 18%, transparent);
        border: 1px solid color-mix(in srgb, var(--md-primary-bg-color) 28%, transparent);
        border-radius: 4px;
        color: var(--md-primary-bg-color);
        cursor: pointer;
        display: inline-flex;
        font: inherit;
        font-size: .64rem;
        gap: .25rem;
        height: 1.6rem;
        line-height: 1;
        margin-left: .35rem;
        padding: 0 .45rem;
      }

      .mattos-random-page:hover,
      .mattos-random-page:focus-visible {
        background: color-mix(in srgb, var(--md-primary-fg-color--light) 28%, transparent);
      }

      .mattos-random-page:disabled {
        cursor: wait;
        opacity: .65;
      }

      @media screen and (max-width: 44.9375em) {
        .mattos-random-page {
          font-size: .6rem;
          padding: 0 .35rem;
        }
      }
    `;
    document.head.appendChild(style);
  }

  function installButton() {
    if (document.getElementById(BUTTON_ID)) {
      return;
    }

    const headerInner = document.querySelector(".md-header__inner");
    if (!headerInner) {
      return;
    }

    addStyles();

    const button = document.createElement("button");
    button.id = BUTTON_ID;
    button.type = "button";
    button.className = "mattos-random-page";
    button.title = "Open a random wiki page";
    button.setAttribute("aria-label", "Open a random wiki page");
    button.textContent = "Random";
    button.addEventListener("click", () => goToRandomPage(button));

    const sourceLink = headerInner.querySelector(".md-header__source");
    if (sourceLink) {
      headerInner.insertBefore(button, sourceLink);
    } else {
      headerInner.appendChild(button);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", installButton);
  } else {
    installButton();
  }

  if (window.document$ && typeof window.document$.subscribe === "function") {
    window.document$.subscribe(installButton);
  }
})();
