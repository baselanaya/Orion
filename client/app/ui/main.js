// Orion — v0.5, NordVPN-style two-pane UI. Vanilla JS. The map is generated
// at runtime from a bundled equirectangular world raster (sampled to dots);
// pins come from real geolocation lookups (ipwho.is) of the home IP and the
// active exit. Every state change maps 1:1 to a helper IPC status.
const __inv = window.__TAURI__ && window.__TAURI__.core ? window.__TAURI__.core.invoke : null;
const invoke = (...a) => __inv
  ? __inv(...a)
  : Promise.reject(new Error("Tauri API unavailable - app must run inside the Orion desktop shell"));
const $ = (id) => document.getElementById(id);
const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

const POOLS = { fast: [10, 49], ghost: [90, 129] };
const MODE_LABEL = { fast: "FAST", ghost: "GHOST", double: "DBL" };

let profiles = [];            // [{name, addr}]
let selected = null;          // profile name chosen in the list
let phase = "standby";        // standby|linking|fast|ghost|fault|offline
let homeLoc = null;           // {lat, lon, city, country} — real home, cached
let exitLoc = null;           // {ip, lat, lon, city, country} — current exit
let nodeGeoCache = {};        // ip -> {city, country, lat, lon}
let lastRx = null, lastTx = null, lastHs = null;
let connecting = false;
let connectedSince = null;
let prevPhase = null;
const NEWNYM_KEY = "orion.newnymConnect";
const AUTO_KEY = "orion.autostart";
let s_activeProfile = null;
let s_nodeIp = null;
let exitTimer = null;

const fmtBytes = (n) => {
  if (n == null) return "--";
  const u = ["B", "KiB", "MiB", "GiB"]; let i = 0, v = n;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return v.toFixed(i ? 1 : 0) + " " + u[i];
};
const fmtAge = (s) => s == null ? "--"
  : s < 90 ? s + "s" : s < 5400 ? Math.round(s / 60) + "m" : Math.round(s / 3600) + "h";

function addrPool(addr) {
  if (!addr) return null;
  const m = addr.match(/^10\.66\.(\d+)\.(\d+)/);
  if (!m) return null;
  const third = +m[1], n = +m[2];
  const fast = POOLS.fast, ghost = POOLS.ghost;
  if (third === 0 || third === 1) {
    if (n >= fast[0] && n <= fast[1]) return "fast";
    if (n >= 50 && n <= 89) return "double";
    if (n >= ghost[0] && n <= ghost[1]) return "ghost";
  }
  return null;
}
const profileByName = (n) => profiles.find((p) => p.name === n) || null;

async function geo(ip) {
  if (!ip) return null;
  if (nodeGeoCache[ip]) return nodeGeoCache[ip];
  try {
    const r = await fetch(`https://ipwho.is/${ip}`, { signal: AbortSignal.timeout(8000) });
    const j = await r.json();
    if (j.success === false) return null;
    const g = {
      ip: j.ip, city: j.city || j.country || "unknown",
      country: j.country || "", lat: j.latitude, lon: j.longitude,
    };
    nodeGeoCache[ip] = g;
    return g;
  } catch { return null; }
}

/* ------------------------------------------------ the map */
const map = (() => {
  const c = $("map");
  const ctx = c.getContext("2d");
  const M = {
    w: 0, h: 0, dots: [], home: null, exit: null, exitP: null,
    connected: false, pulses: [], raf: null,
  };
  const IMG_W = 1280, IMG_H = 640;

  function resize() {
    const r = c.parentElement.getBoundingClientRect();
    const d = devicePixelRatio || 1;
    c.width = Math.max(1, r.width * d);
    c.height = Math.max(1, r.height * d);
    ctx.setTransform(d, 0, 0, d, 0, 0);
    M.w = r.width; M.h = r.height;
    prerender();
    draw();
  }

  let img = null;
  function setSource(image) {
    img = image;
    sample(image);
    prerender();
    draw();
  }
  function sample(image) {
    const off = document.createElement("canvas");
    off.width = IMG_W; off.height = IMG_H;
    const octx = off.getContext("2d", { willReadFrequently: true });
    octx.drawImage(image, 0, 0, IMG_W, IMG_H);
    const px = octx.getImageData(0, 0, IMG_W, IMG_H).data;
    M.dots = [];
    const step = 6;
    for (let sy = 3; sy < IMG_H; sy += step) {
      for (let sx = 3; sx < IMG_W; sx += step) {
        const i = (sy * IMG_W + sx) * 4;
        const r = px[i], g = px[i + 1], b = px[i + 2];
        const isOcean = b > r + 12 && b > g + 4;
        if (!isOcean) M.dots.push({ x: sx, y: sy });
      }
    }
  }
  let layer = null;
  function prerender() {
    if (!M.dots.length || !M.w) { layer = null; return; }
    layer = document.createElement("canvas");
    layer.width = M.w * (devicePixelRatio || 1);
    layer.height = M.h * (devicePixelRatio || 1);
    const l = layer.getContext("2d");
    const dpr = devicePixelRatio || 1;
    l.setTransform(dpr, 0, 0, dpr, 0, 0);
    const scale = Math.max(M.w / IMG_W, M.h / IMG_H);
    const ox = (M.w - IMG_W * scale) / 2;
    const oy = (M.h - IMG_H * scale) / 2;
    l.fillStyle = "rgba(170, 195, 240, 0.34)";
    const s = Math.max(1.8, 1.3 * scale);
    for (const d of M.dots) {
      const x = ox + d.x * scale, y = oy + d.y * scale;
      if (x < -2 || x > M.w + 2 || y < -2 || y > M.h + 2) continue;
      l.fillRect(x, y, s, s);
    }
  }
  function project(lat, lon) {
    const scale = Math.max(M.w / IMG_W, M.h / IMG_H);
    const ox = (M.w - IMG_W * scale) / 2;
    const oy = (M.h - IMG_H * scale) / 2;
    return {
      x: ox + ((lon + 180) / 360) * IMG_W * scale,
      y: oy + ((90 - lat) / 180) * IMG_H * scale,
    };
  }

  function pin(p, color, glow) {
    ctx.fillStyle = color;
    ctx.beginPath(); ctx.arc(p.x, p.y, 4, 0, Math.PI * 2); ctx.fill();
    ctx.strokeStyle = color; ctx.lineWidth = 1.5;
    ctx.beginPath(); ctx.arc(p.x, p.y, 7 + (glow ? 3 : 0), 0, Math.PI * 2); ctx.stroke();
  }

  function draw() {
    ctx.clearRect(0, 0, M.w, M.h);
    if (layer) ctx.drawImage(layer, 0, 0, M.w, M.h);
    if (M.home) {
      const h = project(M.home.lat, M.home.lon);
      pin(h, "rgba(142, 148, 168, 0.9)");
      if (M.exit && M.connected) {
        const e = project(M.exit.lat, M.exit.lon);
        ctx.strokeStyle = "rgba(108, 151, 255, 0.55)";
        ctx.lineWidth = 1.5; ctx.setLineDash([4, 5]);
        ctx.beginPath();
        ctx.moveTo(h.x, h.y);
        const mx = (h.x + e.x) / 2, my = Math.min(h.y, e.y) - 40;
        ctx.quadraticCurveTo(mx, my, e.x, e.y);
        ctx.stroke(); ctx.setLineDash([]);
        pin(e, "#6C97FF", true);
      }
    }
    if (M.exitP && M.connected) {
      for (const p of M.pulses) {
        ctx.strokeStyle = `rgba(108, 151, 255, ${0.5 * (1 - p.t)})`;
        ctx.lineWidth = 1.5;
        ctx.beginPath(); ctx.arc(M.exitP.x, M.exitP.y, 8 + 26 * p.t, 0, Math.PI * 2); ctx.stroke();
      }
    }
  }
  function ensureLoop() {
    if (M.raf) return;
    const loop = () => {
      for (const p of M.pulses) p.t += 0.02;
      M.pulses = M.pulses.filter((p) => p.t < 1);
      draw();
      if (M.pulses.length) M.raf = requestAnimationFrame(loop);
      else M.raf = null;
    };
    M.raf = requestAnimationFrame(loop);
  }

  return {
    resize,
    setSource,
    setHome(loc) { M.home = loc; draw(); },
    setExit(loc, connected) {
      M.exit = loc; M.connected = connected;
      if (loc) M.exitP = project(loc.lat, loc.lon);
      draw();
      if (connected && !reduceMotion) { M.pulses.push({ t: 0 }); ensureLoop(); }
    },
    exitPulse() {
      if (reduceMotion) return;
      if (!M.connected) return;
      M.pulses.push({ t: 0 });
      ensureLoop();
    },
  };
})();

/* ------------------------------------------------ sidebar */
function renderServers() {
  const ul = $("serverList");
  ul.innerHTML = "";
  const ordered = [...profiles].sort((a, b) =>
    (addrPool(a.addr) === "ghost") - (addrPool(b.addr) === "ghost"));
  if (!ordered.length) {
    const li = document.createElement("li");
    li.className = "server";
    const meta = document.createElement("div"); meta.className = "meta";
    const b = document.createElement("b"); b.textContent = "No servers";
    const small = document.createElement("small"); small.textContent = "add a profile to /etc/orion/profiles";
    meta.append(b, small);
    li.append(meta);
    ul.append(li);
    return;
  }
  for (const p of ordered) {
    const pool = addrPool(p.addr);
    const li = document.createElement("li");
    li.className = "server" + (selected === p.name ? " is-selected" : "") +
      (phase !== "standby" && phase !== "fault" && s_activeProfile === p.name ? " is-live" : "");
    li.dataset.profile = p.name;
    const tag = MODE_LABEL[pool] || "SRV";
    let title = "Egress node";
    let sub = "AmneziaWG · direct exit";
    if (pool === "ghost") { title = "Tor network"; sub = "AmneziaWG · random Tor exit"; }
    if (pool === "double") { title = "Via entry"; sub = "AmneziaWG · chained exit"; }
    const nodeIp = s_nodeIp;
    const g = pool !== "ghost" ? nodeGeoCache[nodeIp] : null;
    if (g) { title = g.city || title; sub = `${g.country} · direct exit`; }
    const tagEl = document.createElement("span");
    tagEl.className = "tag mono"; tagEl.textContent = tag;
    const meta = document.createElement("div"); meta.className = "meta";
    const b = document.createElement("b"); b.textContent = title;
    const small = document.createElement("small"); small.textContent = sub;
    meta.append(b, small);
    li.append(tagEl, meta);
    li.onclick = () => {
      if (phase === "linking") return;
      selected = p.name;
      renderServers();
    };
    ul.append(li);
  }
}

/* ------------------------------------------------ state rendering */
const HEADLINES = {
  standby: "You're unprotected",
  linking: "Connecting…",
  fast: "Your traffic is secure & private",
  ghost: "Your traffic is secure & anonymous",
  fault: "Blocked — kill switch active",
  offline: "Helper is down",
};
const SUBS = {
  standby: "Your traffic exits through your home IP. Connect to confine it.",
  linking: "Bringing up the obfuscated tunnel.",
  fast: "Encrypted to your node; exits with its address.",
  ghost: "Exiting via the Tor network; the exit country is random.",
  fault: "The tunnel died; the kill switch sealed your traffic. Reconnect.",
  offline: "Start orion-helper.service to talk to the tunnel.",
};
const CHIP = {
  standby: ["UNPROTECTED", ""], linking: ["CONNECTING", "is-linking"],
  fast: ["SECURED", "is-secured"], ghost: ["SECURED", "is-secured"],
  fault: ["FAULT", "is-fault"], offline: ["OFFLINE", "is-offline"],
};

function render() {
  const connected = phase === "fast" || phase === "ghost";
  const ringSvg = $("ringArc").closest(".ring");

  ringSvg.classList.toggle("is-linking", phase === "linking");
  ringSvg.classList.toggle("is-secured", connected);
  ringSvg.classList.toggle("is-fault", phase === "fault");
  $("ringArc").style.strokeDashoffset =
    connected ? 0 : phase === "fault" ? 94 : 377;

  const power = $("actionBtn");
  power.classList.toggle("is-on", connected);
  power.classList.toggle("is-fault", phase === "fault");
  power.disabled = phase === "linking" || phase === "offline" ||
    (phase === "standby" && !selected);
  const nb = $("newIdBtn");
  if (nb) nb.hidden = !(phase === "ghost");
  const ga = $("ghostActions");
  if (ga) ga.hidden = !(phase === "ghost");
  $("headline").textContent = HEADLINES[phase];
  $("subline").textContent = SUBS[phase];
  if (prevPhase && prevPhase !== phase && phase === "fault") {
    osNotify("Orion", "Tunnel fault detected - the kill switch sealed your traffic.");
  }
  prevPhase = phase;

  const [chip, cls] = CHIP[phase];
  $("statusChip").textContent = chip;
  $("statusDot").parentElement.className = "side-status mono " + cls;

  $("exitChip").style.opacity = connected ? 1 : 0.5;

  renderServers();
}

function renderOffline() {
  phase = "offline";
  render();
}

/* ------------------------------------------------ polling */
async function poll() {
  let s;
  try {
    s = await invoke("status");
  } catch {
    phase = "offline";
    render();
    return;
  }
  try {
    const r = await invoke("list_profiles");
    profiles = r.profiles ?? [];
  } catch { /* keep previous */ }
  if (!selected && profiles.length) selected = profiles[0].name;

  if (connecting) {
    phase = (s.state === "connected")
      ? ((addrPool((profileByName(s.profile) || {}).addr) === "ghost") ? "ghost" : "fast")
      : "linking";
  } else if (s.state === "connected") {
    const p = profileByName(s.profile);
    const pool = addrPool(p && p.addr);
    phase = pool === "ghost" ? "ghost" : "fast";
    s_activeProfile = s.profile;
    s_nodeIp = (s.endpoint || "").split(":")[0] || s_nodeIp;
  } else if (s.state === "locked_no_tunnel") {
    phase = "fault";
  } else {
    phase = "standby";
    s_activeProfile = null;
  }

  $("rx").textContent = fmtBytes(s.rx_bytes);
  $("tx").textContent = fmtBytes(s.tx_bytes);
  $("hs").textContent = fmtAge(s.handshake_age_secs);
  const up = connectedSince ? fmtAge(Math.floor((Date.now() - connectedSince) / 1000)) : "--";
  $("uptime").textContent = up;

  const connected = phase === "fast" || phase === "ghost";
  if (connected && !connectedSince) {
    osNotify("Orion", "Your traffic is now confined through " + (phase === "ghost" ? "Tor." : "your node."));
    connectedSince = Date.now();
    osNotify("Orion", "Your traffic is now confined through " + (phase === "ghost" ? "Tor." : "your node."));
  }
  if (!connected) connectedSince = null;
  if (connected) {
    if (lastHs != null && s.handshake_age_secs != null && s.handshake_age_secs < lastHs) {
      map.exitPulse();
    }
    lastRx = s.rx_bytes; lastTx = s.tx_bytes; lastHs = s.handshake_age_secs;
    await refreshExitGeo();
  } else {
    lastRx = lastTx = lastHs = null;
    setExitText(null);
    if (phase === "standby") await lookupHome();
  }
  render();
  // keep the tray menu + icon in step with reality (Rust diffs internally)
  if (window.__TAURI__) {
    invoke("tray_sync", {
      state: s.state,
      profile: s.profile || null,
      profiles: profiles.map((p) => p.name),
      mode: phase === "fast" || phase === "ghost" ? phase : null,
    }).catch(() => {});
  }
}

async function refreshExitGeo() {
  if (exitTimer) return;
  exitTimer = setTimeout(() => (exitTimer = null), 30000);
  try {
    const g = await geo("");
    if (!g) return;
    exitLoc = g;
    setExitText(g.ip);
    if ((phase === "fast" || phase === "ghost") && g.city) {
      $("subline").textContent = phase === "ghost"
        ? `Exiting via ${g.city}, ${g.country} (Tor).`
        : `Exiting via ${g.city}, ${g.country}.`;
    }
    map.setExit(g, true);
    if (s_nodeIp) await geo(s_nodeIp);
    renderServers();
  } catch { setExitText(null); }
}

async function lookupHome() {
  if (homeLoc) return;
  try {
    const r = await fetch("https://ipwho.is/", { signal: AbortSignal.timeout(8000) });
    const j = await r.json();
    homeLoc = { lat: j.latitude, lon: j.longitude, city: j.city, country: j.country };
    map.setHome(homeLoc);
  } catch { /* no home pin; not critical */ }
}

function setExitText(ip) {
  $("exitText").textContent = ip ? "exit " + ip : "exit --";
  $("copyExit").hidden = !ip;
}

/* ------------------------------------------------ actions */
$("actionBtn").onclick = async () => {
  if (phase === "fast" || phase === "ghost" || phase === "fault") {
    phase = "standby";
    render();
    try { await invoke("disconnect"); } catch { /* status poll reports */ }
  } else if (phase === "standby" && selected) {
    connecting = true;
    phase = "linking";
    render();
    const watchdog = setTimeout(() => {
      if (connecting) {
        connecting = false;
        phase = "standby";
        $("error").textContent = "connection timed out after 30s - check the helper journal";
        render();
      }
    }, 30000);
    try {
      await invoke("connect", { profile: selected });
      setExitText(null);
      if (mode === "ghost" && localStorage.getItem(NEWNYM_KEY) === "1") {
        try {
          await invoke("new_identity");
          exitTimer = null; shownExit = null;
        } catch { /* non-fatal: tunnel is still up */ }
      }
    } catch (e) {
      $("error").textContent = String(e);
      phase = "standby";
    } finally {
      clearTimeout(watchdog);
      connecting = false;
    }
  }
  poll();
};

$("settingsBtn").onclick = () => { $("settingsModal").hidden = false; loadSettings(); };
const newnymBox = $("newnymOnConnect");
if (newnymBox) {
  newnymBox.checked = localStorage.getItem(NEWNYM_KEY) === "1";
  newnymBox.onchange = () => localStorage.setItem(NEWNYM_KEY, newnymBox.checked ? "1" : "0");
}
$("selfTest").onclick = runSelfTest;

/* ---------------------------------------------- settings v0.7 (helper-side) */
let curSettings = null;

function segSet(seg, val) {
  for (const b of seg.querySelectorAll("button")) {
    b.classList.toggle("on", b.dataset.v === val);
  }
}

async function loadSettings() {
  try {
    const s = await invoke("get_settings");
    curSettings = s;
    segSet($("ksSeg"), s.kill_switch);
    segSet($("dnsSeg"), s.dns_mode);
    $("dnsInput").hidden = s.dns_mode !== "custom";
    $("dnsInput").value = s.dns_custom || "";
    $("tbPath").value = s.tor_browser_path || "";
    $("ksHint").hidden = s.kill_switch !== "off";
    $("ksHint").textContent = s.kill_switch === "off"
      ? "kill switch is OFF: with the tunnel down, traffic falls back to your raw network."
      : "strict: with the tunnel down, nothing leaves this machine. allow lan adds local-network reach. off disables the seal.";
    renderPolicyInfo(s);
  } catch (e) {
    $("dnsInfoVal").textContent = "helper unavailable";
    $("ksInfoVal").textContent = "helper unavailable";
  }
}

function renderPolicyInfo(s) {
  $("ksInfoVal").textContent = s.kill_switch === "strict" ? "nftables, fail-closed"
    : s.kill_switch === "allow-lan" ? "nftables, fail-closed + lan" : "OFF - not protected";
  $("dnsInfoVal").textContent = s.dns_mode === "auto" ? (s.dns_hook ? "tunnel-enforced" : "no resolver hook - not enforced")
    : s.dns_mode === "custom" ? "custom: " + (s.dns_custom || "?") : "off (system resolver)";
}

async function saveSettings(fields) {
  try {
    const s = await invoke("set_settings", fields);
    curSettings = s;
    renderPolicyInfo(s);
    $("subline").textContent = "Security settings saved. They apply on the next connect.";
  } catch (e) {
    $("subline").textContent = "settings error: " + e;
    loadSettings();
  }
}

for (const [segId, field] of [["ksSeg", "kill_switch"], ["dnsSeg", "dns_mode"]]) {
  $(segId).addEventListener("click", (e) => {
    const b = e.target.closest("button");
    if (!b) return;
    segSet($(segId), b.dataset.v);
    if (field === "kill_switch") {
      saveSettings({ killSwitch: b.dataset.v });
    } else {
      $("dnsInput").hidden = b.dataset.v !== "custom";
      if (b.dataset.v === "custom" && !$("dnsInput").value.trim()) $("dnsInput").focus();
      saveSettings({ dnsMode: b.dataset.v, dnsCustom: $("dnsInput").value.trim() || null });
    }
  });
}
$("dnsInput").onchange = () => {
  const v = $("dnsInput").value.trim();
  if (v) saveSettings({ dnsMode: "custom", dnsCustom: v });
};
$("tbPath").onchange = () => saveSettings({ torBrowserPath: $("tbPath").value.trim() });

async function runSelfTest() {
  const btn = $("selfTest"), out = $("diagResults");
  btn.disabled = true;
  out.innerHTML = "";
  const row = (name, ok, detail) => {
    const d = document.createElement("div");
    d.className = "row";
    d.innerHTML = `<span>${name}</span><span class="${ok ? "ok" : "fail"}">${ok ? "PASS" : "FAIL"}${detail ? " - " + detail : ""}</span>`;
    out.append(d);
  };
  row("helper link", !!(window.__TAURI__ && window.__TAURI__.core));
  let st = null;
  try { st = await invoke("status"); row("helper status", true, st.state); } catch (e) { row("helper status", false, String(e)); btn.disabled = false; return; }
  let stg = null;
  try { stg = await invoke("get_settings"); } catch { /* optional */ }
  if (stg) {
    row("resolver hook", stg.dns_hook, stg.dns_hook ? "dns can be enforced" : "install openresolv or enable systemd-resolved");
    row("kill switch policy", stg.kill_switch !== "off", stg.kill_switch);
    row("dns policy", stg.dns_mode !== "off", stg.dns_mode + (stg.dns_custom ? ": " + stg.dns_custom : ""));
  }
  if (st.detail) row("last warning", false, st.detail);
  try {
    const g = await geo("");
    row("exit ip", true, g ? `${g.ip} (${g.city}, ${g.country})` : "no lookup");
  } catch { row("exit ip", false, "lookup failed"); }
  try {
    const r = await fetch("https://cloudflare-dns.com/dns-query?name=example.com&type=A", { headers: { accept: "application/dns-json" }, signal: AbortSignal.timeout(6000) });
    const j = await r.json();
    row("dns resolution", (j.Answer || []).length > 0, (j.Answer?.[0]?.data) || "no answer");
  } catch { row("dns resolution", false, "no answer - leak or block"); }
  row("handshake", st.handshake_age_secs != null && st.handshake_age_secs < 180,
      st.handshake_age_secs != null ? st.handshake_age_secs + "s ago" : "none yet");
  btn.disabled = false;
}
$("settingsClose").onclick = () => { $("settingsModal").hidden = true; };
$("settingsModal").addEventListener("click", (e) => {
  if (e.target === $("settingsModal")) $("settingsModal").hidden = true;
});
addEventListener("keydown", (e) => {
  if (e.key === "Escape") $("settingsModal").hidden = true;
});

$("newIdBtn").onclick = async () => {
  if (phase !== "ghost") return;
  $("newIdBtn").disabled = true;
  try {
    await invoke("new_identity");
    $("subline").textContent = "New identity: Tor circuits rotated.";
    exitTimer = null; shownExit = null;
    await refreshExitGeo();
  } catch (e) {
    $("subline").textContent = "New identity failed: " + e;
  }
  render();
};

$("copyExit").onclick = () => {
  if (exitLoc) navigator.clipboard.writeText(exitLoc.ip);
};

addEventListener("resize", () => map.resize());

/* ------------------------------------------------ tray events */
if (window.__TAURI__ && window.__TAURI__.event) {
  const { listen } = window.__TAURI__.event;
  listen("tray-new-id", () => { if (!$("newIdBtn").hidden) $("newIdBtn").click(); });
  listen("tray-error", (e) => { $("error").textContent = String(e.payload || e); });
  listen("tray-tb", (e) => {
    $("subline").textContent = e.payload ? "Tor Browser launched: its own Tor rides our Ghost tunnel." : "Tor Browser not found - set its path in Settings.";
  });
}

$("tbBtn").onclick = async () => {
  try {
    const r = await invoke("launch_tor_browser");
    if (r.state === "launched") {
      $("subline").textContent = "Tor Browser launched: its own Tor rides our Ghost tunnel.";
    } else {
      $("subline").textContent = "Tor Browser not found - set its path in Settings.";
    }
  } catch (e) { $("subline").textContent = String(e); }
};

$("aliasBtn").onclick = async () => {
  const key = localStorage.getItem("orion.slKey");
  if (!key) { $("subline").textContent = "set a SimpleLogin API key in Settings first."; return; }
  try {
    const r = await fetch("https://app.simplelogin.io/api/alias/random/new?hostname=orion", {
      method: "POST",
      headers: { Authentication: key },
      signal: AbortSignal.timeout(10000),
    });
    const j = await r.json();
    if (j.alias) {
      await navigator.clipboard.writeText(j.alias);
      $("subline").textContent = "alias created + copied: " + j.alias;
    } else {
      $("subline").textContent = "alias error: " + (j.error || "unknown");
    }
  } catch (e) { $("subline").textContent = "alias error: " + e; }
};

const slKey = $("slKey");
slKey.value = localStorage.getItem("orion.slKey") || "";
slKey.onchange = () => localStorage.setItem("orion.slKey", slKey.value.trim());

/* ------------------------------------------------ boot */
const worldImg = new Image();
worldImg.onload = () => map.setSource(worldImg);
worldImg.src = "assets/world.jpg";
map.resize();
render();
poll();
setInterval(poll, 2000);
lookupHome();
// auto-start: connect once the first status poll confirms standby
if (autoStart) {
  const autoTimer = setInterval(() => {
    if (phase === "standby" && selected && !connecting) {
      clearInterval(autoTimer);
      $("actionBtn").click();
    }
  }, 1200);
  setTimeout(() => clearInterval(autoTimer), 20000);
}
