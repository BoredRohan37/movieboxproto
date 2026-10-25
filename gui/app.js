/* MovieBox OTT front-end — vanilla JS, zero frameworks.
   Fast enough for software-rendered low-spec machines. */
"use strict";

/* ============================ utilities ============================ */

const $ = (sel, el) => (el || document).querySelector(sel);
const $$ = (sel, el) => Array.from((el || document).querySelectorAll(sel));

const API = {
  async get(path) {
    const r = await fetch(path, { headers: { Accept: "application/json" } });
    if (!r.ok) {
      let msg = r.statusText;
      try { msg = (await r.json()).error || msg; } catch (e) {}
      throw new Error(msg);
    }
    return r.json();
  },
  async post(path, body) {
    const r = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body || {}),
    });
    if (!r.ok) {
      let msg = r.statusText;
      try { msg = (await r.json()).error || msg; } catch (e) {}
      throw new Error(msg);
    }
    return r.json();
  },
  async del(path) {
    const r = await fetch(path, { method: "DELETE" });
    if (!r.ok) throw new Error(r.statusText);
    return r.json();
  },
};

function posterUrl(url, width) {
  if (!url) return "";
  const w = width || (GUI.data.data_saver ? 220 : 342);
  return "/api/poster?w=" + w + "&u=" + encodeURIComponent(url);
}

function esc(s) {
  return String(s == null ? "" : s)
    .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}

function fmtBytes(n) {
  if (!n) return "";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0, v = n;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return v.toFixed(v >= 10 || i === 0 ? 0 : 1) + " " + units[i];
}

function fmtTime(sec) {
  sec = Math.max(0, Math.floor(sec || 0));
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  const mm = String(m).padStart(2, "0"), ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${m}:${ss}`;
}

function toast(msg, kind) {
  const box = $("#toasts");
  const el = document.createElement("div");
  el.className = "toast" + (kind ? " " + kind : "");
  el.textContent = msg;
  box.appendChild(el);
  setTimeout(() => { el.style.opacity = "0"; el.style.transition = "opacity .3s"; }, 3400);
  setTimeout(() => el.remove(), 3800);
}

/* Lazy image loader via IntersectionObserver (falls back to immediate). */
const imgObserver = "IntersectionObserver" in window
  ? new IntersectionObserver((entries) => {
      for (const en of entries) {
        if (en.isIntersecting) {
          hydrateImg(en.target);
          imgObserver.unobserve(en.target);
        }
      }
    }, { rootMargin: "420px" })
  : null;

function hydrateImg(el) {
  const src = el.getAttribute("data-src");
  if (!src) return;
  el.removeAttribute("data-src");
  el.src = src;
  el.onload = () => el.classList.add("loaded");
  el.onerror = () => { el.classList.add("loaded"); el.style.opacity = "0.08"; };
}

function imgTag(url, alt, cls) {
  const src = url ? posterUrl(url) : "";
  if (!src) return `<img class="${cls || ""}" alt="" style="opacity:.08">`;
  const el = `<img data-src="${esc(src)}" alt="${esc(alt || "")}" ${cls ? `class="${cls}"` : ""} loading="lazy" decoding="async">`;
  return el;
}

function observeImgs(root) {
  $$("img[data-src]", root).forEach((el) => {
    if (imgObserver) imgObserver.observe(el);
    else hydrateImg(el);
  });
}

/* ============================ settings store ============================ */

const GUI = {
  data: { theme: "dark", max_height: 1080, data_saver: false, reduce_motion: false, kids_mode: false, autoplay_next: true, default_caption: "" },
  async load() {
    try {
      const r = await API.get("/api/settings");
      Object.assign(this.data, r.settings || {});
    } catch (e) {}
    this.apply();
  },
  async save(patch) {
    Object.assign(this.data, patch);
    this.apply();
    try { await API.post("/api/settings", patch); } catch (e) {}
  },
  apply() {
    document.body.classList.toggle("light", this.data.theme === "light");
    document.body.classList.toggle("reduce-motion", !!this.data.reduce_motion);
  },
};

/* ============================ item helpers ============================ */

function itemKey(it) {
  return (it.provider && (it.provider.cache_key || it.provider)) || it.provider || "moviebox";
}

/* Catalog items serialize `id` as {provider: "moviebox", value: "…"}.
   History/favorites serialize provider as a plain string. */
function provOf(it) {
  if (it.id && typeof it.id === "object") return String(it.id.provider || "moviebox");
  return typeof it.provider === "string" ? it.provider : "moviebox";
}

function idOf(it) {
  if (it.id && typeof it.id === "object") return it.id.value;
  if (it.id != null) return String(it.id);
  return it.subject_id || "";
}

function hrefOf(it) {
  const stype = it.stype || (String(it.media_type || "").toLowerCase() === "series" ? 2 : 1);
  return `#/title/${provOf(it)}/${encodeURIComponent(idOf(it))}?stype=${stype}`;
}

function titleOf(it) { return it.title || "(untitled)"; }
function yearOf(it) { return it.year || it.release_year || ""; }
function posterOf(it) { return it.poster_url || it.cover_url || ""; }

function cardHtml(it, opts) {
  opts = opts || {};
  const badge = opts.badge ? `<span class="card-badge">${esc(opts.badge)}</span>` : "";
  const prog = opts.progress
    ? `<span class="card-progress"><i style="width:${Math.min(100, Math.round(opts.progress * 100))}%"></i></span>`
    : "";
  const sub = opts.sub ? `<div class="card-sub">${esc(opts.sub)}</div>` : "";
  return `<a class="card" href="${hrefOf(it)}">
    <div class="card-poster">${badge}${imgTag(posterOf(it), titleOf(it))}${prog}</div>
    <div class="card-title">${esc(titleOf(it))}</div>
    ${sub}
  </a>`;
}

function rowHtml(id, title, items, opts) {
  opts = opts || {};
  if (!items || !items.length) return "";
  const spark = opts.spark ? `<span class="spark">◆</span>` : "";
  const more = opts.moreHref ? `<a class="row-more" href="${opts.moreHref}">See all ›</a>` : "";
  const cards = items.map((it, i) => cardHtml(it, Object.assign({}, opts.cardOpts, opts.perItem ? opts.perItem(it, i) : null))).join("");
  return `<section class="row ${opts.rank ? "rank-row" : ""}" id="${esc(id)}">
    <div class="row-head"><h2 class="row-title">${spark}${esc(title)}</h2>${more}</div>
    <div class="row-scroller">${cards}</div>
  </section>`;
}

function emptyHtml(icon, text) {
  return `<div class="empty"><div class="big">${icon}</div><div>${esc(text)}</div></div>`;
}

/* ============================ router ============================ */

const routes = [];
function route(pattern, handler) {
  routes.push({ pattern, handler });
}

let currentCleanup = null;
async function render() {
  if (typeof currentCleanup === "function") { try { currentCleanup(); } catch (e) {} currentCleanup = null; }
  const hash = location.hash || "#/";
  const [pathPart, queryPart] = hash.slice(1).split("?");
  const path = pathPart || "/";
  const query = new URLSearchParams(queryPart || "");
  const view = $("#view");
  window.scrollTo(0, 0);

  for (const r of routes) {
    const m = path.match(r.pattern);
    if (m) {
      try {
        currentCleanup = await r.handler(view, m, query);
      } catch (e) {
        console.error(e);
        view.innerHTML = emptyHtml("⚠", e.message || "Something went wrong.");
      }
      markNav(path);
      observeImgs(view);
      return;
    }
  }
  view.innerHTML = emptyHtml("🎬", "Page not found.");
  markNav(path);
}

function markNav(path) {
  const key = path === "/" ? "home" : path.split("/")[1] || "home";
  $$("#mainnav a").forEach((a) => a.classList.toggle("active", a.dataset.nav === key));
}

window.addEventListener("hashchange", render);

/* ============================ home ============================ */

route(/^\/$/, async (view) => {
  view.innerHTML = `<div class="hero skeleton" style="min-height:340px"></div>` +
    Array.from({ length: 3 }, () => `<section class="row"><div class="row-head"><h2 class="row-title skeleton" style="width:180px;height:20px;border-radius:6px"></h2></div><div class="row-scroller">${'<div class="skeleton" style="width:152px;aspect-ratio:2/3;border-radius:12px"></div>'.repeat(7)}</div></section>`).join("");

  const [home, ai] = await Promise.all([
    API.get("/api/home").catch(() => null),
    API.get("/api/ai/recommendations").catch(() => null),
  ]);

  const sections = (home && home.sections) || [];
  let html = "";

  // --- hero: featured item(s) from the first sections ---
  const featured = sections.flatMap((s) => s.items).filter((i) => posterOf(i)).slice(0, 5);
  if (featured.length) {
    html += heroHtml(featured);
  }

  // --- continue watching (history) ---
  try {
    const h = await API.get("/api/history");
    const items = (h.items || []).map((it) => ({
      id: it.subject_id, title: it.title, provider: it.provider,
      poster_url: it.cover_url, year: it.release_year, stype: it.stype,
    }));
    // dedupe by subject (history has per-episode rows)
    const seen = new Set();
    const uniq = items.filter((i) => (seen.has(i.id) ? false : (seen.add(i.id), true)));
    const progMap = {};
    (h.items || []).forEach((it) => {
      const p = it.duration_seconds ? it.progress_seconds / it.duration_seconds : 0;
      progMap[it.subject_id] = Math.max(progMap[it.subject_id] || 0, p || 0);
    });
    html += rowHtml("row-cw", "Continue Watching", uniq.slice(0, 14), {
      spark: true,
      moreHref: "#/history",
      perItem: (it) => ({ progress: progMap[it.id] || 0, sub: (it.stype === 2 ? "Series" : "Movie") }),
    });
  } catch (e) {}

  // --- AI rows (ScoredItem serializes flattened: fields live on the item) ---
  if (ai && ai.for_you && ai.for_you.length) {
    html += rowHtml("row-foryou", "For You — AI picks", ai.for_you, {
      spark: true, moreHref: "#/ai",
      perItem: (r) => ({ sub: r.reason || "" }),
    });
  }
  if (ai && ai.because_you_watched) {
    for (const row of ai.because_you_watched.slice(0, 2)) {
      html += rowHtml("row-byw", `Because you watched ${row.anchor}`, row.items, {
        spark: true,
      });
    }
  }
  // --- provider sections ---
  sections.forEach((s, idx) => {
    const isTop10 = idx === 0 && /top|trend|popular|most|featured/i.test(s.title);
    if (isTop10) {
      html += rowHtml("row-top10", "Top 10 Today", s.items.slice(0, 10), {
        rank: true, spark: true,
        perItem: (it, i) => ({ badge: String(i + 1) }),
      });
    } else {
      html += rowHtml("row-" + idx, s.title, s.items.slice(0, 20), {});
    }
  });

  // --- mood chips ---
  html += `<section class="row"><div class="row-head"><h2 class="row-title"><span class="spark">◆</span>Browse by mood</h2></div>
    <div class="chip-row" id="mood-chips">
      <a class="chip" href="#/mood/feel-good">Feel-Good</a>
      <a class="chip" href="#/mood/adrenaline">Adrenaline</a>
      <a class="chip" href="#/mood/mind-bending">Mind-Bending</a>
      <a class="chip" href="#/mood/spooky">Spooky</a>
      <a class="chip" href="#/mood/cozy">Cozy</a>
      <a class="chip" href="#/mood/tearjerker">Tearjerkers</a>
      <a class="chip" href="#/mood/laugh-out-loud">Laugh Out Loud</a>
      <a class="chip" href="#/mood/true-crime">True Crime</a>
      <a class="chip" href="#/mood/epic">Epics</a>
      <a class="chip" href="#/mood/anime-vibes">Anime</a>
    </div></section>`;

  view.innerHTML = html;
  setupHero(featured);
  return cleanupHome;
});

function cleanupHome() { if (heroTimer) clearInterval(heroTimer); heroTimer = null; }

let heroTimer = null;
function heroHtml(items) {
  const slides = items.map((it, i) => `
    <div class="hero-slide" data-idx="${i}" style="position:absolute;inset:0;opacity:${i === 0 ? 1 : 0};transition:opacity .5s ease">
      <img class="hero-img" data-src="${esc(posterUrl(posterOf(it), 728))}" alt="">
      <div class="hero-scrim"></div>
    </div>`).join("");
  return `<section class="hero" id="hero">
    ${slides}
    <div class="hero-content" id="hero-content"></div>
    <div class="hero-dots">${items.map((_, i) => `<button data-i="${i}" class="${i === 0 ? "on" : ""}" aria-label="Slide ${i + 1}"></button>`).join("")}</div>
  </section>`;
}

function setupHero(items) {
  const hero = $("#hero");
  if (!hero || !items.length) return;
  const content = $("#hero-content");
  let idx = 0;

  const show = (i) => {
    idx = i % items.length;
    $$(".hero-slide", hero).forEach((s, j) => { s.style.opacity = j === idx ? 1 : 0; });
    $$(".hero-dots button", hero).forEach((b, j) => b.classList.toggle("on", j === idx));
    const it = items[idx];
    const stype = it.stype || (String(it.media_type || "").toLowerCase() === "series" ? 2 : 1);
    content.innerHTML = `
      <div class="hero-kicker">${esc(it.year || "")} · ${stype === 2 ? "Series" : "Movie"} · Featured</div>
      <h1 class="hero-title">${esc(titleOf(it))}</h1>
      <div class="hero-actions">
        <a class="btn btn-play" href="#/watch/${provOf(it)}/${encodeURIComponent(idOf(it))}?stype=${stype}&autoplay=1">▶ Play</a>
        <a class="btn btn-ghost" href="${hrefOf(it)}">ⓘ More Info</a>
      </div>`;
    observeImgs(content.parentElement || content);
    observeImgs(hero);
  };

  show(0);
  heroTimer = setInterval(() => show(idx + 1), 7000);
  $$(".hero-dots button", hero).forEach((b) =>
    b.addEventListener("click", () => { clearInterval(heroTimer); show(+b.dataset.i); heroTimer = setInterval(() => show(idx + 1), 7000); })
  );
}

/* ============================ quick search ============================ */

function setupQuickSearch() {
  const overlay = $("#qs-overlay");
  const input = $("#qs-input");
  const results = $("#qs-results");
  let t = null;

  const open = () => { overlay.hidden = false; input.value = ""; results.innerHTML = ""; input.focus(); };
  const close = () => { overlay.hidden = true; };

  $("#quick-search-btn").addEventListener("click", open);
  overlay.addEventListener("click", (e) => { if (e.target === overlay) close(); });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !overlay.hidden) close();
    const tag = (document.activeElement && document.activeElement.tagName) || "";
    if (e.key === "/" && tag !== "INPUT" && tag !== "TEXTAREA" && overlay.hidden) {
      e.preventDefault(); open();
    }
  });

  input.addEventListener("input", () => {
    clearTimeout(t);
    const q = input.value.trim();
    if (q.length < 2) { results.innerHTML = ""; return; }
    t = setTimeout(async () => {
      try {
        const [sug, ai] = await Promise.all([
          API.get("/api/suggest?q=" + encodeURIComponent(q)).catch(() => ({ suggestions: [] })),
          API.get("/api/ai/search?q=" + encodeURIComponent(q)).catch(() => null),
        ]);
        let html = "";
        const aiItems = ((ai && ai.results) || []).slice(0, 4).map((r) => r.item);
        if (ai && ai.interpreted) {
          html += `<div class="ai-note"><b>AI:</b> ${esc(ai.interpreted)}</div>`;
        }
        const seen = new Set();
        const merged = [...aiItems, ...(sug.suggestions || []).map((s) => ({ title: s, suggestOnly: true }))];
        for (const it of merged) {
          if (it.suggestOnly) {
            html += `<div class="qs-item" data-suggest="${esc(it.title)}"><div class="t">⌕ ${esc(it.title)}</div></div>`;
            continue;
          }
          const k = idOf(it);
          if (seen.has(k)) continue;
          seen.add(k);
          html += `<a class="qs-item" href="${hrefOf(it)}">
            ${imgTag(posterOf(it), titleOf(it))}
            <div><div class="t">${esc(titleOf(it))}</div><div class="s">${esc(yearOf(it))}</div></div></a>`;
        }
        if (!html) html = `<div class="ai-note">No matches — try different words, or ask the AI Hub.</div>`;
        results.innerHTML = html;
        observeImgs(results);
        $$(".qs-item[data-suggest]", results).forEach((el) =>
          el.addEventListener("click", () => { close(); location.hash = "#/search?q=" + encodeURIComponent(el.dataset.suggest); })
        );
      } catch (e) { /* silent */ }
    }, 260);
  });
}

/* ============================ boot ============================ */

async function boot() {
  await GUI.load();
  setupQuickSearch();
  try {
    const h = await API.get("/api/health");
    $("#ver").textContent = "v" + h.version;
  } catch (e) {}
  render();
}

boot();
