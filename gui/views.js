/* MovieBox views: browse, search, my list, history, mood, details, player,
   AI hub, settings. Loaded after app.js (shares its helpers). */
"use strict";

/* ============================ browse ============================ */

route(/^\/browse$/, async (view, m, q) => {
  const preset = q.get("preset") || "trending";
  const tabs = [
    ["trending", "Trending Now"],
    ["top", "Top Rated"],
    ["recent", "New & Fresh"],
    ["most", "Most Watched"],
  ];
  view.innerHTML = `<div class="page">
    <h1 class="page-title">Browse</h1>
    <div class="chip-row">${tabs.map(([id, label]) =>
      `<a class="chip ${id === preset ? "on" : ""}" href="#/browse?preset=${id}">${label}</a>`).join("")}
    </div>
    <div id="browse-grid" class="grid" style="margin-top:18px">
      ${'<div class="skeleton" style="aspect-ratio:2/3;border-radius:12px"></div>'.repeat(18)}
    </div></div>`;
  const data = await API.get("/api/browse?preset=" + encodeURIComponent(preset));
  const grid = $("#browse-grid");
  grid.innerHTML = data.items && data.items.length
    ? data.items.map((it) => cardHtml(it)).join("")
    : emptyHtml("🍿", "Nothing here right now.");
  observeImgs(grid);
});

/* ============================ mood ============================ */

route(/^\/mood\/([\w-]+)$/, async (view, m) => {
  const id = m[1];
  view.innerHTML = `<div class="page"><h1 class="page-title">AI Mood</h1>
    <div class="grid">${'<div class="skeleton" style="aspect-ratio:2/3;border-radius:12px"></div>'.repeat(12)}</div></div>`;
  const data = await API.get("/api/ai/mood?id=" + encodeURIComponent(id) + "&limit=36");
  view.innerHTML = `<div class="page">
    <div class="page-sub"><b style="color:var(--gold)">◆ AI mood collection</b></div>
    <h1 class="page-title">${esc(data.label || "Mood")}</h1>
    <div class="grid">${(data.items || []).map((r) => cardHtml(r)).join("") || emptyHtml("🎭", "No titles learned for this mood yet.")}</div></div>`;
  observeImgs(view);
});

/* ============================ search ============================ */

route(/^\/search$/, async (view, m, q) => {
  const initial = q.get("q") || "";
  view.innerHTML = `<div class="search-wrap">
    <input id="search-input" class="search-input" type="search" placeholder="Search titles — or describe a vibe, like “funny space adventure”"
      value="${esc(initial)}" autocomplete="off" spellcheck="false">
    <div class="sug-list" id="sug-list"></div>
    <div id="search-results" style="margin-top:22px"></div>
  </div>`;
  const input = $("#search-input");
  const sugList = $("#sug-list");
  const results = $("#search-results");
  let t = null;

  async function doSearch(qs) {
    if (!qs) { results.innerHTML = ""; sugList.innerHTML = ""; return; }
    results.innerHTML = `<div class="grid">${'<div class="skeleton" style="aspect-ratio:2/3;border-radius:12px"></div>'.repeat(12)}</div>`;
    try {
      const [sr, ai] = await Promise.all([
        API.get("/api/search?q=" + encodeURIComponent(qs)),
        API.get("/api/ai/search?q=" + encodeURIComponent(qs)).catch(() => null),
      ]);
      let html = "";
      if (ai && ai.interpreted) {
        html += `<div class="ai-note"><b>AI understood:</b> ${esc(ai.interpreted)}</div>`;
      }
      const seen = new Set();
      const merge = [];
      if (ai) {
        for (const r of ai.results || []) {
          const k = idOf(r); // flattened ScoredItem
          if (!seen.has(k)) { seen.add(k); merge.push(Object.assign({}, r, { _reason: r.reason })); }
        }
        for (const it of ai.provider_results || []) {
          const k = idOf(it);
          if (!seen.has(k)) { seen.add(k); merge.push(it); }
        }
      }
      for (const it of sr.items || []) {
        const k = idOf(it);
        if (!seen.has(k)) { seen.add(k); merge.push(it); }
      }
      html += `<div class="grid">${merge.map((it) => cardHtml(it, it._reason ? { sub: it._reason } : null)).join("") || emptyHtml("🔍", "No matches found.")}</div>`;
      results.innerHTML = html;
      observeImgs(results);
    } catch (e) {
      results.innerHTML = emptyHtml("⚠", e.message);
    }
  }

  async function doSuggest(qs) {
    if (qs.length < 2) { sugList.innerHTML = ""; return; }
    try {
      const s = await API.get("/api/suggest?q=" + encodeURIComponent(qs));
      sugList.innerHTML = (s.suggestions || []).slice(0, 6).map((sg) =>
        `<div class="sug" data-q="${esc(sg)}">⌕ ${esc(sg)}</div>`).join("");
      $$(".sug", sugList).forEach((el) => el.addEventListener("click", () => {
        input.value = el.dataset.q; doSearch(el.dataset.q); sugList.innerHTML = "";
      }));
    } catch (e) {}
  }

  input.addEventListener("input", () => {
    clearTimeout(t);
    const v = input.value.trim();
    t = setTimeout(() => { doSearch(v); doSuggest(v); }, 320);
  });
  if (initial) doSearch(initial);
  else input.focus();
});

/* ============================ my list ============================ */

route(/^\/list$/, async (view) => {
  view.innerHTML = `<div class="page"><h1 class="page-title">My List</h1>
    <div class="grid">${'<div class="skeleton" style="aspect-ratio:2/3;border-radius:12px"></div>'.repeat(8)}</div></div>`;
  const data = await API.get("/api/favorites");
  const items = (data.items || []).map((f) => ({
    id: f.subject_id, title: f.title, provider: f.provider,
    poster_url: f.cover_url, year: f.release_year, stype: f.stype,
  }));
  view.innerHTML = `<div class="page">
    <h1 class="page-title">My List</h1>
    <p class="page-sub">${items.length} saved titles</p>
    ${items.length ? `<div class="grid">${items.map((it) => cardHtml(it)).join("")}</div>` : emptyHtml("❤", "Your list is empty — tap ♥ on any title.")}
  </div>`;
  observeImgs(view);
});

/* ============================ history ============================ */

route(/^\/history$/, async (view) => {
  view.innerHTML = `<div class="page"><h1 class="page-title">History</h1>
    <div class="grid">${'<div class="skeleton" style="aspect-ratio:2/3;border-radius:12px"></div>'.repeat(8)}</div></div>`;
  const data = await API.get("/api/history");
  const rows = data.items || [];
  const seen = new Set();
  const items = [];
  const meta = {};
  for (const it of rows) {
    if (seen.has(it.subject_id)) { continue; }
    seen.add(it.subject_id);
    items.push({
      id: it.subject_id, title: it.title, provider: it.provider,
      poster_url: it.cover_url, year: it.release_year, stype: it.stype,
      season: it.season, episode: it.episode,
    });
    meta[it.subject_id] = it;
  }
  view.innerHTML = `<div class="page">
    <div style="display:flex;align-items:center;justify-content:space-between">
      <h1 class="page-title">History</h1>
      ${rows.length ? '<button class="btn btn-ghost btn-sm" id="clear-hist">Clear all</button>' : ""}
    </div>
    ${items.length ? `<div class="grid">${items.map((it) => {
      const m2 = meta[it.id];
      const p = m2.duration_seconds ? Math.min(1, m2.progress_seconds / m2.duration_seconds) : 0;
      const sub = m2.completed ? "Finished" : (m2.season ? `S${m2.season}E${m2.episode} · ` : "") + (p ? Math.round(p * 100) + "% watched" : "Started");
      return cardHtml(it, { progress: p, sub });
    }).join("")}</div>` : emptyHtml("🕒", "Nothing watched yet.")}
  </div>`;
  observeImgs(view);
  const clear = $("#clear-hist");
  if (clear) clear.addEventListener("click", async () => {
    if (!confirm("Clear all watch history?")) return;
    await API.del("/api/history");
    toast("History cleared", "ok");
    render();
  });
});

/* ============================ details ============================ */

route(/^\/title\/([\w-]+)\/(.+)$/, async (view, m, q) => {
  const provider = m[1];
  const id = decodeURIComponent(m[2]);
  const stype = +(q.get("stype") || 1);
  view.innerHTML = `<div class="page">
    <div class="detail-card"><div class="detail-poster skeleton" style="aspect-ratio:2/3"></div>
    <div class="detail-info"><div class="skeleton" style="height:34px;width:60%;border-radius:8px"></div>
    <div class="skeleton" style="height:60px;border-radius:8px"></div></div></div></div>`;

  const data = await API.get(`/api/details?provider=${encodeURIComponent(provider)}&id=${encodeURIComponent(id)}`);
  const d = data.details;
  const isSeries = d.is_series || (d.seasons && d.seasons.length) || stype === 2;

  // resume point
  const hist = data.history || [];
  const resume = hist.length ? hist[0] : null;

  let seasonNum = resume ? resume.season : (d.seasons && d.seasons.length ? d.seasons[0].number : 0);
  let episodeNum = resume ? resume.episode : 1;

  const genres = (d.genres || []).map((g) => `<a class="chip" href="#/search?q=${encodeURIComponent(g)}">${esc(g)}</a>`).join("");

  view.innerHTML = `<div class="page">
    <div class="detail-card">
      <div class="detail-poster">${imgTag(d.poster_url, d.title)}</div>
      <div class="detail-info">
        <h1 class="detail-title">${esc(d.title)}</h1>
        ${d.tagline ? `<div class="detail-tagline">${esc(d.tagline)}</div>` : ""}
        <div class="meta-row">
          ${d.year ? `<span class="pill">${esc(d.year)}</span>` : ""}
          <span class="pill">${isSeries ? "Series" : "Movie"}</span>
          ${d.imdb_rating ? `<span class="pill gold">★ ${esc(d.imdb_rating)}</span>` : ""}
          ${d.duration ? `<span class="pill">${esc(d.duration)}</span>` : ""}
          ${d.prints ? `<span class="pill">${esc(d.prints)}</span>` : ""}
        </div>
        ${d.description ? `<p class="detail-desc">${esc(d.description)}</p>` : ""}
        ${genres ? `<div class="genre-chips">${genres}</div>` : ""}
        <div class="spec-grid">
          ${d.director ? `<span>Director <b>${esc(d.director)}</b></span>` : ""}
          ${d.stars ? `<span>Starring <b>${esc(d.stars)}</b></span>` : ""}
          ${d.audios ? `<span>Audio <b>${esc(d.audios)}</b></span>` : ""}
        </div>
        <div class="detail-actions">
          <button class="btn btn-play" id="btn-play">▶ ${resume && !resume.completed && resume.progress_seconds > 30 ? "Resume" : "Play"}</button>
          <button class="btn btn-ghost" id="btn-fav">${data.favorite ? "♥ In My List" : "♡ My List"}</button>
          ${isSeries && d.seasons && d.seasons.length ? `<select class="p-select" id="season-select" style="color:var(--text)"></select>` : ""}
        </div>
        ${resume && !resume.completed && resume.progress_seconds > 30 ? `<div class="ai-note">Resuming <b>${esc(d.title)}</b>${resume.season ? ` · S${resume.season}E${resume.episode}` : ""} — about ${fmtTime((resume.duration_seconds || 0) - resume.progress_seconds)} left</div>` : ""}
      </div>
    </div>
    <div id="episodes"></div>
    <div id="similar"></div>
  </div>`;
  observeImgs(view);

  // season select
  const seasonSel = $("#season-select");
  if (seasonSel && d.seasons && d.seasons.length) {
    seasonSel.innerHTML = d.seasons.map((s) => `<option value="${s.number}" ${s.number === seasonNum ? "selected" : ""}>Season ${s.number}</option>`).join("");
    seasonSel.addEventListener("change", () => {
      seasonNum = +seasonSel.value; episodeNum = 1;
      renderEpisodes();
    });
  }

  function watchHref(season, episode, releaseIdx) {
    return `#/watch/${provider}/${encodeURIComponent(id)}?stype=${isSeries ? 2 : 1}` +
      `&season=${season || 0}&episode=${episode || 0}` +
      (releaseIdx != null ? `&release=${releaseIdx}` : "") +
      `&title=${encodeURIComponent(d.title)}`;
  }

  function renderEpisodes() {
    const box = $("#episodes");
    if (!isSeries || !d.seasons || !d.seasons.length) { box.innerHTML = ""; return; }
    const season = d.seasons.find((s) => s.number === seasonNum) || d.seasons[0];
    const watchedSet = new Set(hist.map((h) => h.episode + ":" + (h.completed ? "1" : "0")));
    box.innerHTML = `<h2 class="section-h">Episodes — Season ${season.number}</h2>
      <div class="epi-list">${(season.episodes || []).map((ep) => {
        const wasWatched = hist.some((h) => h.episode === ep.number && h.season === season.number && h.completed);
        return `<div class="epi ${wasWatched ? "watched" : ""}" data-ep="${ep.number}">
          <div class="epi-num">${ep.number}</div>
          <div style="min-width:0">
            <div class="epi-t">${esc(ep.title || `Episode ${ep.number}`)} ${wasWatched ? "✓" : ""}</div>
            ${ep.overview ? `<div class="epi-d">${esc(ep.overview)}</div>` : ""}
          </div></div>`;
      }).join("")}</div>`;
    $$(".epi", box).forEach((el) => el.addEventListener("click", () => {
      episodeNum = +el.dataset.ep;
      openPlayer(watchHref(seasonNum, episodeNum));
    }));
  }

  renderEpisodes();

  // similar (AI taste row for this title's genres) — flattened ScoredItems
  fetch("/api/ai/recommendations").then((r) => r.json()).then((ai) => {
    const box = $("#similar");
    const rows = (ai.because_you_watched || []).find((b) => b.anchor === d.title);
    const items = rows ? rows.items : (ai.for_you || []).slice(0, 12);
    if (items.length) {
      box.innerHTML = rowHtml("similar-row", "More like this", items, { spark: true });
      observeImgs(box);
    }
  }).catch(() => {});

  // favorite toggle
  $("#btn-fav").addEventListener("click", async (e) => {
    const btn = e.currentTarget;
    const adding = !data.favorite;
    if (adding) {
      await API.post("/api/favorites", {
        provider, id, title: d.title, cover_url: d.poster_url,
        stype: isSeries ? 2 : 1, year: d.year || "",
      });
    } else {
      await API.del(`/api/favorites?provider=${encodeURIComponent(provider)}&id=${encodeURIComponent(id)}`);
    }
    data.favorite = adding;
    btn.textContent = adding ? "♥ In My List" : "♡ My List";
    toast(adding ? "Added to My List" : "Removed from My List", "ok");
  });

  // play (movie: direct; series: current season/episode)
  $("#btn-play").addEventListener("click", () => openPlayer(watchHref(seasonNum, episodeNum)));

  function openPlayer(href) {
    if (resume && !resume.completed && resume.progress_seconds > 30 &&
        (!isSeries || (resume.season === seasonNum && resume.episode === episodeNum))) {
      location.hash = href + "&t=" + Math.floor(resume.progress_seconds);
    } else {
      location.hash = href;
    }
  }
});

/* ============================ player ============================ */

route(/^\/watch\/([\w-]+)\/(.+)$/, async (view, m, q) => {
  const provider = m[1];
  const id = decodeURIComponent(m[2]);
  const stype = +(q.get("stype") || 1);
  const season = +(q.get("season") || 0);
  const episode = +(q.get("episode") || 0);
  const title = q.get("title") || "";
  const startT = +(q.get("t") || 0);
  let releaseIdx = q.get("release") != null ? +q.get("release") : null;
  let captions = [];
  let saveTimer = null;
  let uiTimer = null;
  let currentResolve = null;
  let releases = [];

  view.innerHTML = `<div class="player-page" id="player-page">
    <div class="player-stage" id="stage">
      <div class="player-loading" id="p-loading">
        <div class="spinner"></div>
        <div>Preparing stream…</div>
        <div class="player-error" id="p-error"></div>
      </div>
    </div>
    <div class="player-top">
      <button class="p-btn" id="p-back" title="Back (Esc)">✕</button>
      <div class="pt-title">${esc(title)}${season ? ` · S${String(season).padStart(2, "0")}E${String(episode).padStart(2, "0")}` : ""}</div>
    </div>
    <div class="player-bottom">
      <div class="seekbar" id="seekbar">
        <div class="track"></div><div class="buffered" id="p-buffered"></div>
        <div class="played" id="p-played"></div><div class="knob" id="p-knob" style="left:0"></div>
      </div>
      <div class="p-controls">
        <button class="p-btn" id="p-play" title="Play/Pause (k)">▶</button>
        <button class="p-btn" id="p-rw" title="Back 10s (←)">«10s</button>
        <button class="p-btn" id="p-fw" title="Forward 10s (→)">10s»</button>
        <span class="p-time" id="p-time">0:00 / 0:00</span>
        <span class="p-spacer"></span>
        <select class="p-select" id="p-quality" title="Quality" style="display:none"></select>
        <select class="p-select" id="p-captions" title="Subtitles" style="display:none"><option value="">CC Off</option></select>
        <select class="p-select" id="p-speed" title="Speed">
          <option value="0.5">0.5×</option><option value="0.75">0.75×</option>
          <option value="1" selected>1×</option><option value="1.25">1.25×</option>
          <option value="1.5">1.5×</option><option value="2">2×</option>
        </select>
        <button class="p-btn" id="p-fs" title="Fullscreen (f)">⛶</button>
      </div>
    </div>
  </div>`;

  const page = $("#player-page");
  const stage = $("#stage");
  let video = null;
  let dashPlayer = null;

  function showError(msg) {
    $("#p-loading").style.display = "flex";
    $(".spinner", page).style.display = "none";
    $("#p-loading").firstElementChild.nextElementSibling.textContent = "Playback problem";
    $("#p-error").textContent = msg;
  }

  async function resolveStream(ri) {
    const params = new URLSearchParams({
      provider, id, season: String(season), episode: String(episode),
      release: String(ri == null ? 0 : ri),
      max_height: String(GUI.data.max_height || 1080),
      title,
    });
    const r = await API.get("/api/resolve?" + params.toString());
    return r;
  }

  async function loadReleases() {
    try {
      const params = new URLSearchParams({ provider, id, season: String(season), episode: String(episode) });
      const r = await API.get("/api/releases?" + params.toString());
      releases = r.releases || [];
      const sel = $("#p-quality");
      if (releases.length > 1) {
        sel.style.display = "";
        sel.innerHTML = releases.map((rel, i) =>
          `<option value="${i}" ${releaseIdx === i ? "selected" : ""}>${esc(rel.quality || rel.filename || ("Source " + (i + 1)))}${rel.size_bytes ? " · " + fmtBytes(rel.size_bytes) : ""}</option>`).join("");
      } else {
        sel.style.display = "none";
      }
    } catch (e) { /* quality list optional */ }
  }

  async function loadCaptions(resourceId) {
    try {
      const params = new URLSearchParams({ provider, id, season: String(season), episode: String(episode) });
      if (resourceId) params.set("resource", resourceId);
      const r = await API.get("/api/captions?" + params.toString());
      captions = r.captions || [];
      const sel = $("#p-captions");
      if (captions.length) {
        sel.style.display = "";
        for (const c of captions) {
          const opt = document.createElement("option");
          opt.value = c.url;
          opt.textContent = c.name || "Subtitle";
          sel.appendChild(opt);
        }
      } else {
        sel.style.display = "none";
      }
    } catch (e) { /* captions optional */ }
  }

  function attachCaptions() {
    if (!video) return;
    $$("track", video).forEach((t) => t.remove());
    const sel = $("#p-captions");
    const url = sel.value;
    if (!url) return;
    const track = document.createElement("track");
    track.kind = "subtitles";
    track.label = "Subtitles";
    track.default = true;
    track.src = "/api/caption-file?u=" + encodeURIComponent(url);
    video.appendChild(track);
    if (video.textTracks && video.textTracks[0]) video.textTracks[0].mode = "showing";
  }

  async function start(ri, keepTime) {
    $("#p-loading").style.display = "flex";
    $(".spinner", page).style.display = "";
    try {
      currentResolve = await resolveStream(ri);
      if (releaseIdx == null) releaseIdx = ri || 0;
    } catch (e) {
      showError(e.message || "Could not resolve stream.");
      return;
    }

    // teardown previous
    if (dashPlayer) { try { dashPlayer.reset(); } catch (e) {} dashPlayer = null; }
    if (video) { video.pause(); video.removeAttribute("src"); try { video.load(); } catch (e) {} video.remove(); video = null; }

    const t0 = keepTime || startT || 0;

    if (currentResolve.dash) {
      // Load dash.js on demand (cached after first load).
      await new Promise((resolve, reject) => {
        if (window.dashjs) return resolve();
        const s = document.createElement("script");
        s.src = "/assets/dash.min.js";
        s.onload = resolve;
        s.onerror = () => reject(new Error("DASH engine failed to load"));
        document.head.appendChild(s);
      });
      video = document.createElement("video");
      video.playsInline = true;
      video.preload = "auto";
      stage.appendChild(video);
      dashPlayer = window.dashjs.MediaPlayer().create();
      dashPlayer.initialize(video, location.origin + currentResolve.play_url, true);
      if (t0) dashPlayer.seek(t0);
    } else {
      video = document.createElement("video");
      video.playsInline = true;
      video.preload = "auto";
      video.src = currentResolve.play_url;
      if (t0) video.currentTime = t0;
      stage.appendChild(video);
      video.play().catch(() => { /* user gesture needed */ });
    }

    video.addEventListener("loadedmetadata", () => {
      $("#p-loading").style.display = "none";
      updateUi();
      attachCaptions();
    });
    video.addEventListener("timeupdate", onTimeUpdate);
    video.addEventListener("progress", onProgress);
    video.addEventListener("play", () => { $("#p-play").textContent = "⏸"; });
    video.addEventListener("pause", () => { $("#p-play").textContent = "▶"; saveProgress(false); });
    video.addEventListener("ended", onEnded);
    video.addEventListener("error", () => showError("The stream could not be played. Try another quality or source."));

    loadCaptions(currentResolve.resource_id);
    startSaveTimer();
  }

  function onTimeUpdate() {
    if (!video) return;
    updateUi();
  }

  function onProgress() {
    if (!video || !video.buffered || !video.buffered.length) return;
    const end = video.buffered.end(video.buffered.length - 1);
    const dur = video.duration || 1;
    $("#p-buffered").style.width = Math.min(100, (end / dur) * 100) + "%";
  }

  function updateUi() {
    if (!video || !video.duration) return;
    const pct = (video.currentTime / video.duration) * 100;
    $("#p-played").style.width = pct + "%";
    $("#p-knob").style.left = pct + "%";
    $("#p-time").textContent = `${fmtTime(video.currentTime)} / ${fmtTime(video.duration)}`;
  }

  function saveProgress(completed) {
    if (!video || !video.duration) return;
    const payload = {
      provider, id, title,
      cover_url: null,
      stype,
      year: "",
      season, episode,
      progress_seconds: Math.floor(video.currentTime || 0),
      duration_seconds: Math.floor(video.duration || 0),
      completed: !!completed || (video.duration && video.currentTime / video.duration > 0.95),
    };
    API.post("/api/history/progress", payload).catch(() => {});
  }

  function startSaveTimer() {
    stopSaveTimer();
    saveTimer = setInterval(() => {
      if (video && !video.paused && video.duration) saveProgress(false);
    }, 10000);
  }
  function stopSaveTimer() { if (saveTimer) clearInterval(saveTimer); saveTimer = null; }

  async function onEnded() {
    saveProgress(true);
    // Autoplay next episode
    if (GUI.data.autoplay_next && stype === 2) {
      try {
        const d = await API.get(`/api/details?provider=${encodeURIComponent(provider)}&id=${encodeURIComponent(id)}`);
        const s = (d.details.seasons || []).find((x) => x.number === season);
        const next = s && (s.episodes || []).find((ep) => ep.number === episode + 1);
        if (next) {
          toast(`Next: Episode ${next.number}`, "ok");
          location.hash = `#/watch/${provider}/${encodeURIComponent(id)}?stype=2&season=${season}&episode=${next.number}&title=${encodeURIComponent(title)}`;
          return;
        }
      } catch (e) {}
    }
  }

  /* controls */
  $("#p-back").addEventListener("click", () => history.back());
  $("#p-play").addEventListener("click", togglePlay);
  function togglePlay() { if (!video) return; if (video.paused) video.play().catch(() => {}); else video.pause(); }
  $("#p-rw").addEventListener("click", () => { if (video) video.currentTime = Math.max(0, video.currentTime - 10); });
  $("#p-fw").addEventListener("click", () => { if (video) video.currentTime = Math.min(video.duration || 1e9, video.currentTime + 10); });
  $("#p-fs").addEventListener("click", toggleFs);
  function toggleFs() {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    else (stage.requestFullscreen && stage.requestFullscreen()).catch?.(() => {});
  }
  $("#p-speed").addEventListener("change", (e) => { if (video) video.playbackRate = +e.target.value; });
  $("#p-captions").addEventListener("change", attachCaptions);
  $("#p-quality").addEventListener("change", async (e) => {
    const ri = +e.target.value;
    const t = video ? video.currentTime : 0;
    releaseIdx = ri;
    await start(ri, t);
  });

  /* seekbar interaction */
  const seekbar = $("#seekbar");
  let seeking = false;
  function seekTo(clientX) {
    if (!video || !video.duration) return;
    const rect = seekbar.getBoundingClientRect();
    const pct = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    video.currentTime = pct * video.duration;
    updateUi();
  }
  seekbar.addEventListener("mousedown", (e) => { seeking = true; seekTo(e.clientX); });
  seekbar.addEventListener("mousemove", (e) => { if (seeking) seekTo(e.clientX); });
  document.addEventListener("mouseup", () => { seeking = false; });
  seekbar.addEventListener("touchstart", (e) => seekTo(e.touches[0].clientX), { passive: true });
  seekbar.addEventListener("touchmove", (e) => seekTo(e.touches[0].clientX), { passive: true });

  /* ui auto-hide */
  function pokeUi() {
    page.classList.remove("player-ui-hidden");
    clearTimeout(uiTimer);
    uiTimer = setTimeout(() => { if (video && !video.paused) page.classList.add("player-ui-hidden"); }, 3200);
  }
  stage.addEventListener("mousemove", pokeUi);
  stage.addEventListener("click", (e) => {
    if (e.target === video) togglePlay();
    pokeUi();
  });
  pokeUi();

  /* keyboard */
  function onKey(e) {
    if (e.key === " " || e.key === "k") { e.preventDefault(); togglePlay(); }
    else if (e.key === "ArrowLeft" || e.key === "j") video && (video.currentTime = Math.max(0, video.currentTime - 10));
    else if (e.key === "ArrowRight" || e.key === "l") video && (video.currentTime += 10);
    else if (e.key === "f") toggleFs();
    else if (e.key === "m") { if (video) video.muted = !video.muted; }
    else if (e.key === "c") { const s = $("#p-captions"); if (s.style.display !== "none") { s.value = s.value ? "" : (s.options[1] ? s.options[1].value : ""); attachCaptions(); } }
    else if (e.key === "Escape") history.back();
    pokeUi();
  }
  document.addEventListener("keydown", onKey);

  window.addEventListener("hashchange", teardown, { once: true });
  function teardown() {
    stopSaveTimer();
    clearTimeout(uiTimer);
    document.removeEventListener("keydown", onKey);
    if (video && video.duration) saveProgress(false);
    if (dashPlayer) { try { dashPlayer.reset(); } catch (e) {} dashPlayer = null; }
    if (video) { video.pause(); video.removeAttribute("src"); try { video.load(); } catch (e) {} }
    fetch("/api/stop-playback").catch(() => {});
  }
  window.addEventListener("beforeunload", () => { if (video && video.duration) saveProgress(false); });

  /* boot the stream: auto-pick release matching data_saver / max_height */
  await loadReleases();
  let autoIdx = releaseIdx;
  if (autoIdx == null) {
    autoIdx = 0;
    const cap = GUI.data.data_saver ? 480 : (GUI.data.max_height || 1080);
    const wanted = parseInt(String(cap), 10);
    let best = null;
    releases.forEach((rel, i) => {
      const q = parseInt(String(rel.quality || "").replace(/\D/g, ""), 10) || 0;
      if (q && q <= wanted && (!best || q > best.q)) best = { i, q };
    });
    autoIdx = best ? best.i : 0;
  }
  start(autoIdx, startT);

  return teardown;
});

/* ============================ AI hub ============================ */

route(/^\/ai$/, async (view) => {
  view.innerHTML = `<div class="page">
    <h1 class="page-title">AI Hub</h1>
    <p class="page-sub">Your private cinema, with a local AI concierge.</p>
    <div id="ai-stats"></div>
    <div class="ai-grid">
      <div class="chat-panel">
        <div class="chat-head"><span class="dot"></span> MovieBox AI <span id="chat-src" style="font-size:11px;color:var(--faint)"></span></div>
        <div class="chat-log" id="chat-log"></div>
        <div class="chat-sugs" id="chat-sugs"></div>
        <div class="chat-input">
          <input id="chat-msg" placeholder="Ask anything — “what should I watch tonight?”" autocomplete="off">
          <button id="chat-send">Send</button>
        </div>
      </div>
      <div id="ai-side">
        <h2 class="section-h">Mood collections</h2>
        <div class="chip-row" id="ai-moods"></div>
        <h2 class="section-h">Your taste profile</h2>
        <div id="ai-profile" class="ai-note">Learning…</div>
        <h2 class="section-h">Top genres</h2>
        <div class="genre-bars" id="ai-genres"></div>
      </div>
    </div>
  </div>`;

  // stats + profile
  try {
    const [ins, rec] = await Promise.all([
      API.get("/api/ai/insights"),
      API.get("/api/ai/recommendations"),
    ]);
    const i = ins.insights;
    $("#ai-stats").innerHTML = `<div class="stat-cards">
      <div class="stat-card"><div class="stat-v">${i.unique_titles}</div><div class="stat-l">Titles explored</div></div>
      <div class="stat-card"><div class="stat-v">${(i.watch_seconds / 3600).toFixed(1)}h</div><div class="stat-l">Watch time</div></div>
      <div class="stat-card"><div class="stat-v">${i.completed}</div><div class="stat-l">Completed</div></div>
      <div class="stat-card"><div class="stat-v">${i.streak_days}🔥</div><div class="stat-l">Day streak</div></div>
    </div>`;
    $("#ai-profile").innerHTML = rec.cold_start
      ? "Play or favorite a few titles — the AI learns fast."
      : esc(rec.profile_summary);
    const genres = (i.favorite_genres || []);
    const maxW = Math.max(0.001, ...genres.map((g) => g[1]));
    $("#ai-genres").innerHTML = genres.map(([g, w]) =>
      `<div class="gbar"><span>${esc(g)}</span><span class="bar"><i style="width:${Math.round((w / maxW) * 100)}%"></i></span><span class="w">${w.toFixed(1)}</span></div>`).join("")
      || `<div class="ai-note">No genre signals yet.</div>`;
  } catch (e) {}

  // moods
  const moodIds = [["feel-good","Feel-Good"],["adrenaline","Adrenaline"],["mind-bending","Mind-Bending"],["spooky","Spooky"],["cozy","Cozy"],["tearjerker","Tearjerkers"],["laugh-out-loud","Laugh Out Loud"],["true-crime","True Crime"],["epic","Epics"],["anime-vibes","Anime"]];
  $("#ai-moods").innerHTML = moodIds.map(([id, label]) => `<a class="chip" href="#/mood/${id}">${label}</a>`).join("");

  // chat
  const log = $("#chat-log");
  const input = $("#chat-msg");
  const sugs = $("#chat-sugs");

  function addMsg(cls, text, source) {
    const el = document.createElement("div");
    el.className = "msg " + cls;
    el.textContent = text;
    if (source) {
      const s = document.createElement("span");
      s.className = "src";
      s.textContent = source;
      el.appendChild(s);
    }
    log.appendChild(el);
    log.scrollTop = log.scrollHeight;
    return el;
  }

  function setSuggestions(list) {
    sugs.innerHTML = (list || []).map((s) => `<button class="chip" data-s="${esc(s)}">${esc(s)}</button>`).join("");
    $$("button", sugs).forEach((b) => b.addEventListener("click", () => { input.value = b.dataset.s; send(); }));
  }

  function addCards(items) {
    if (!items || !items.length) return;
    const el = document.createElement("div");
    el.className = "chat-items";
    el.innerHTML = items.slice(0, 6).map((it) => cardHtml(it)).join("");
    log.appendChild(el);
    observeImgs(el);
    log.scrollTop = log.scrollHeight;
  }

  let busy = false;
  async function send() {
    const msg = input.value.trim();
    if (!msg || busy) return;
    busy = true;
    input.value = "";
    addMsg("user", msg);
    const typing = addMsg("bot", "…");
    try {
      const r = await API.post("/api/ai/chat", { message: msg });
      typing.remove();
      addMsg("bot", r.reply.answer, r.reply.source === "llm" ? "LLM-enhanced" : "local AI");
      setSuggestions(r.reply.suggestions);
      addCards(r.reply.items);
    } catch (e) {
      typing.textContent = "Sorry — " + (e.message || "chat failed");
    }
    busy = false;
    input.focus();
  }

  $("#chat-send").addEventListener("click", send);
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") send(); });

  addMsg("bot", "Hi! I'm your MovieBox AI. I learn from what you watch and can pick the perfect title for any mood. What are you in the mood for?", "local AI");
  setSuggestions(["What should I watch tonight?", "Something funny and short", "Continue watching", "Show my viewing stats"]);

  try {
    const h = await API.get("/api/health");
    $("#chat-src").textContent = h.ai && h.ai.llm ? "LLM connected" : "offline mode";
  } catch (e) {}
});

/* ============================ settings ============================ */

route(/^\/settings$/, async (view) => {
  const [gui, ai] = await Promise.all([
    API.get("/api/settings").catch(() => ({ settings: {} })),
    API.get("/api/ai/settings").catch(() => ({ settings: {} })),
  ]);
  const g = Object.assign({}, GUI.data, gui.settings || {});
  const a = (ai.settings || {});

  view.innerHTML = `<div class="page">
    <h1 class="page-title">Settings</h1>
    <div class="settings-grid">
      <div class="set-card">
        <h3>Playback</h3>
        <p class="hint">Tuned for your hardware — lower quality uses less CPU on software rendering.</p>
        <div class="set-row"><label>Max quality</label>
          <select id="set-maxh">
            ${[360, 480, 720, 1080].map((h) => `<option value="${h}" ${+g.max_height === h ? "selected" : ""}>${h}p</option>`).join("")}
          </select></div>
        <div class="set-row"><label>Data saver</label>
          <span class="switch"><input type="checkbox" id="set-saver" ${g.data_saver ? "checked" : ""}><i></i></span></div>
        <div class="set-row"><label>Autoplay next episode</label>
          <span class="switch"><input type="checkbox" id="set-autonext" ${g.autoplay_next ? "checked" : ""}><i></i></span></div>
      </div>
      <div class="set-card">
        <h3>Appearance</h3>
        <p class="hint">Light theme and reduced motion for older GPUs.</p>
        <div class="set-row"><label>Theme</label>
          <select id="set-theme">
            <option value="dark" ${g.theme !== "light" ? "selected" : ""}>Dark</option>
            <option value="light" ${g.theme === "light" ? "selected" : ""}>Light</option>
          </select></div>
        <div class="set-row"><label>Reduce motion</label>
          <span class="switch"><input type="checkbox" id="set-motion" ${g.reduce_motion ? "checked" : ""}><i></i></span></div>
        <div class="set-row"><label>Kids mode (coming soon)</label>
          <span class="switch"><input type="checkbox" disabled ${g.kids_mode ? "checked" : ""}><i></i></span></div>
      </div>
      <div class="set-card">
        <h3>AI — local engine</h3>
        <p class="hint">Recommendations, semantic search, moods and insights run fully offline.</p>
        <div class="set-row"><label>Local AI</label>
          <span class="switch"><input type="checkbox" checked disabled><i></i></span></div>
      </div>
      <div class="set-card">
        <h3>AI — assistant upgrade (optional)</h3>
        <p class="hint">Connect any OpenAI-compatible endpoint to upgrade chat answers. Key stays on this device.</p>
        <div class="set-row"><label>Enable</label>
          <span class="switch"><input type="checkbox" id="ai-enabled" ${a.llm_enabled ? "checked" : ""}><i></i></span></div>
        <div class="set-row"><label>Endpoint</label>
          <input type="text" id="ai-endpoint" placeholder="https://api.openai.com/v1" value="${esc(a.llm_endpoint || "")}"></div>
        <div class="set-row"><label>Model</label>
          <input type="text" id="ai-model" placeholder="gpt-4o-mini" value="${esc(a.llm_model || "")}"></div>
        <div class="set-row"><label>API key</label>
          <input type="password" id="ai-key" placeholder="sk-…" value=""></div>
        <div class="set-row"><label></label>
          <button class="btn btn-play btn-sm" id="ai-save">Save AI settings</button></div>
      </div>
    </div>
  </div>`;

  $("#set-maxh").addEventListener("change", (e) => GUI.save({ max_height: +e.target.value }));
  $("#set-saver").addEventListener("change", (e) => GUI.save({ data_saver: e.target.checked }));
  $("#set-autonext").addEventListener("change", (e) => GUI.save({ autoplay_next: e.target.checked }));
  $("#set-theme").addEventListener("change", (e) => GUI.save({ theme: e.target.value }));
  $("#set-motion").addEventListener("change", (e) => GUI.save({ reduce_motion: e.target.checked }));

  $("#ai-save").addEventListener("click", async () => {
    try {
      await API.post("/api/ai/settings", {
        llm_enabled: $("#ai-enabled").checked,
        llm_endpoint: $("#ai-endpoint").value,
        llm_model: $("#ai-model").value,
        llm_api_key: $("#ai-key").value,
      });
      toast("AI settings saved", "ok");
    } catch (e) {
      toast(e.message || "Failed to save", "err");
    }
  });
});
