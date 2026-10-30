const REPO = 'nxtime/papercraft';

const REDUCED = matchMedia('(prefers-reduced-motion: reduce)').matches;

const ICONS = {
  windows: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M3 5.5 10.5 4.4v7.1H3V5.5Zm0 13 7.5 1.1v-7H3v5.9Zm8.4 1.2L21 21V12.6h-9.6v7.1Zm0-15.4v7.2H21V3l-9.6 1.3Z"/></svg>',
  macos: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M16.4 12.6c0-2.4 2-3.6 2.1-3.7-1.1-1.7-2.9-1.9-3.5-1.9-1.5-.2-2.9.9-3.7.9-.8 0-1.9-.9-3.2-.8-1.6 0-3.1 1-4 2.4-1.7 3-.4 7.3 1.2 9.7.8 1.2 1.8 2.5 3 2.4 1.2 0 1.7-.8 3.1-.8 1.5 0 1.9.8 3.2.8 1.3 0 2.2-1.2 3-2.4.9-1.4 1.3-2.7 1.3-2.8 0 0-2.5-1-2.5-3.8ZM14 5.4c.7-.8 1.1-1.9 1-3-.9 0-2.1.6-2.8 1.4-.6.7-1.2 1.9-1 2.9 1.1.1 2.1-.5 2.8-1.3Z"/></svg>',
  linux: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2c-2 0-3.3 1.7-3.3 4.2 0 1.4.4 2.4-.4 3.7C7 12 5.5 14 5.5 16.3c0 .6.1 1.1.4 1.6-.9.4-2 .9-2 1.8 0 1.2 1.9 1.3 3.5 1.8 1.3.4 2 .5 2.6-.3.4.1 1.2.2 2 .2s1.6-.1 2-.2c.6.8 1.3.7 2.6.3 1.6-.5 3.5-.6 3.5-1.8 0-.9-1.1-1.4-2-1.8.3-.5.4-1 .4-1.6 0-2.3-1.5-4.3-2.8-6.4-.8-1.3-.4-2.3-.4-3.7C15.3 3.7 14 2 12 2Zm-1.4 4c.4 0 .7.5.7 1.1s-.3 1-.7 1-.7-.4-.7-1 .3-1.1.7-1.1Zm2.8 0c.4 0 .7.5.7 1.1s-.3 1-.7 1-.7-.4-.7-1 .3-1.1.7-1.1ZM12 9c.9 0 2 .6 2 1s-1.1 1-2 1-2-.6-2-1 1.1-1 2-1Z"/></svg>'
};

const PLATFORMS = [
  { key: 'windows', name: 'Windows', arch: 'x86_64', note: 'Choose the MSI installer, standalone app, full portable bundle, CLI tools or relay.' },
  { key: 'macos', name: 'macOS', arch: 'Apple silicon', note: 'PaperCraft.app in a disk image, plus the CLI tools.' },
  { key: 'linux', name: 'Linux', arch: 'x86_64 · X11 and Wayland', note: 'Needs Vulkan or GL and the usual X11 libraries.' }
];

const OS = (() => {
  const ua = navigator.userAgent;

  if (/Windows/i.test(ua)) return 'windows';
  if (/Macintosh|Mac OS X/i.test(ua) && !/iPhone|iPad/i.test(ua)) return 'macos';
  if (/Linux/i.test(ua) && !/Android/i.test(ua)) return 'linux';
  return null;
})();

const toast = message => {
  const el = document.getElementById('toast');

  el.textContent = message;
  el.classList.add('on');
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => el.classList.remove('on'), 1800);
};

const copy = (text, label) => navigator.clipboard.writeText(text)
  .then(() => toast(`${label} copied`))
  .catch(() => toast('Copy was blocked. Select the text instead.'));

const escapeHtml = s => s.replace(/[&<>"']/g, c => `&#${c.charCodeAt(0)};`);

/* Release: release.json is written by the site workflow; the GitHub API is the fallback. */

const formatSize = bytes => `${(bytes / 1048576).toFixed(1)} MB`;

const formatDate = iso => new Date(iso).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });

const fileType = name => (name.endsWith('.tar.gz') ? 'tar.gz' : name.split('.').pop());

const fileLabel = name => {
  if (name.includes('-relay-')) return 'Relay server';
  if (name.includes('-cli-')) return 'CLI + MCP tools';
  if (name.endsWith('.msi')) return 'Installer';
  if (name.includes('-portable.exe')) return 'Portable app';
  if (name.includes('-portable.zip')) return 'Portable bundle';
  return 'Desktop app';
};

const loadRelease = async () => {
  try {
    const res = await fetch('release.json', { cache: 'no-cache' });

    if (res.ok) return await res.json();
  } catch (err) {
    console.warn('release.json is unavailable, asking the GitHub API', err);
  }

  const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`);

  if (!res.ok) throw new Error(`The GitHub API answered ${res.status}`);
  const r = await res.json();

  return {
    tag: r.tag_name,
    name: r.name,
    publishedAt: r.published_at,
    url: r.html_url,
    assets: r.assets.map(a => ({ name: a.name, size: a.size, url: a.browser_download_url, digest: a.digest || null }))
  };
};

const renderRelease = release => {
  const grid = document.getElementById('platforms');

  document.querySelectorAll('[data-version]').forEach(el => { el.textContent = release.tag; });
  document.getElementById('release-date').textContent = formatDate(release.publishedAt);
  document.getElementById('release-link').href = release.url;

  grid.innerHTML = PLATFORMS.map(p => {
    const files = release.assets.filter(a => a.name.includes(`-${p.key}-`));
    const mine = p.key === OS;
    const rows = files.map(a => `
      <div class="file">
        <a href="${escapeHtml(a.url)}">
          <span class="fname">${fileLabel(a.name)}</span>
          <span class="fmeta">${fileType(a.name)} · ${formatSize(a.size)}</span>
        </a>
        ${a.digest ? `<button class="sha" type="button" data-digest="${escapeHtml(a.digest)}" title="Copy the SHA-256 checksum">SHA-256</button>` : ''}
      </div>`).join('');

    return `
      <article class="platform acrylic lit reveal${mine ? ' mine' : ''}">
        ${mine ? '<span class="tag">Your system</span>' : ''}
        <div class="os">${ICONS[p.key]}<div><h3>${p.name}</h3><div class="arch">${p.arch}</div></div></div>
        ${rows || '<p class="note">No build in this release. Build from source.</p>'}
        <p class="note">${p.note}</p>
      </article>`;
  }).join('');

  grid.querySelectorAll('.sha').forEach(btn => {
    btn.addEventListener('click', () => copy(btn.dataset.digest.replace(/^sha256:/, ''), 'Checksum'));
  });
  grid.querySelectorAll('.reveal').forEach(el => revealer.observe(el));
  grid.querySelectorAll('.lit').forEach(light);

  const platformAssets = OS ? release.assets.filter(a => a.name.includes(`-${OS}-`)) : [];
  const primary = platformAssets.find(a => a.name.endsWith('.msi'))
    || platformAssets.find(a => a.name.includes('-portable.exe'))
    || platformAssets.find(a => !a.name.includes('-cli-') && !a.name.includes('-relay-'));

  if (primary) {
    document.getElementById('hero-dl').href = primary.url;
    document.getElementById('hero-dl-label').textContent = `Download for ${PLATFORMS.find(p => p.key === OS).name}`;
  }
};

const showReleaseError = err => {
  console.error(err);
  document.getElementById('platforms').innerHTML = `
    <article class="platform acrylic">
      <h3>The release list did not load</h3>
      <p class="note">Every build is on the GitHub releases page.</p>
      <a class="btn btn-ghost" href="https://github.com/${REPO}/releases/latest">Open releases</a>
    </article>`;
};

/* Motion */

const revealer = new IntersectionObserver(entries => {
  entries.forEach(e => {
    if (!e.isIntersecting) return;
    e.target.classList.add('in-view');
    revealer.unobserve(e.target);
  });
}, { rootMargin: '0px 0px -8% 0px' });

const light = el => {
  el.addEventListener('pointermove', e => {
    const r = el.getBoundingClientRect();

    el.style.setProperty('--lx', `${e.clientX - r.left}px`);
    el.style.setProperty('--ly', `${e.clientY - r.top}px`);
  });
};

const splitHeadline = () => {
  const h1 = document.getElementById('headline');
  let i = 0;

  const wrap = node => {
    if (node.nodeType === Node.TEXT_NODE) {
      const frag = document.createDocumentFragment();

      node.textContent.split(/(\s+)/).forEach(part => {
        if (!part) return;
        if (/^\s+$/.test(part)) { frag.append(part); return; }
        const span = document.createElement('span');

        span.className = 'w';
        span.style.setProperty('--i', i++);
        span.textContent = part;
        frag.append(span);
      });
      node.replaceWith(frag);
      return;
    }
    [...node.childNodes].forEach(wrap);
  };

  [...h1.childNodes].forEach(wrap);
};

const onScroll = () => {
  const y = scrollY;
  const max = document.documentElement.scrollHeight - innerHeight;
  const win = document.getElementById('window');
  const rect = win.getBoundingClientRect();
  const t = Math.min(1, Math.max(0, 1 - rect.top / (innerHeight * 0.8)));

  document.getElementById('nav').classList.toggle('scrolled', y > 24);
  document.querySelector('.progress').style.setProperty('--p', max > 0 ? (y / max).toFixed(4) : 0);

  if (!REDUCED && innerWidth > 640) {
    win.style.setProperty('--tilt', `${(10 * (1 - t)).toFixed(2)}deg`);
    win.style.setProperty('--zoom', (0.96 + 0.04 * t).toFixed(4));
  }
};

const tiltShots = () => {
  document.querySelectorAll('.tilt').forEach(box => {
    const shot = box.querySelector('.shot');

    box.addEventListener('pointermove', e => {
      const r = box.getBoundingClientRect();
      const x = (e.clientX - r.left) / r.width - 0.5;
      const y = (e.clientY - r.top) / r.height - 0.5;

      shot.style.setProperty('--ry', `${(x * 6).toFixed(2)}deg`);
      shot.style.setProperty('--rx', `${(-y * 6).toFixed(2)}deg`);
    });
    box.addEventListener('pointerleave', () => {
      shot.style.setProperty('--ry', '0deg');
      shot.style.setProperty('--rx', '0deg');
    });
  });
};

const countUp = el => {
  const to = Number(el.dataset.to);
  const start = performance.now();

  const step = now => {
    const k = Math.min(1, (now - start) / 1200);

    el.textContent = Math.round(to * (1 - Math.pow(1 - k, 3)));
    if (k < 1) requestAnimationFrame(step);
  };

  requestAnimationFrame(step);
};

/* The live agent demo: each tool call lights up, then the artboard shows its effect. */
const runDemo = () => {
  const calls = [...document.querySelectorAll('#calls .call')];
  const board = document.getElementById('board');
  const pill = document.getElementById('mode-pill');
  const undo = document.getElementById('undo-count');
  const flash = document.getElementById('flash');
  const lint = document.getElementById('lint');
  const effects = [
    () => board.classList.add('s1'),
    () => board.classList.add('s2'),
    () => board.classList.add('s3'),
    () => board.classList.add('s4'),
    () => { board.classList.add('light'); pill.textContent = 'Mode: Light'; },
    () => { flash.classList.remove('on'); void flash.offsetWidth; flash.classList.add('on'); },
    () => lint.classList.add('on')
  ];
  let step = 0;
  let timer = null;

  const reset = () => {
    step = 0;
    calls.forEach(c => c.classList.remove('run', 'done'));
    board.className = 'board';
    pill.textContent = 'Mode: Dark';
    lint.classList.remove('on');
    undo.textContent = '0 undo steps';
  };

  const tick = () => {
    if (step === calls.length) {
      timer = setTimeout(() => { reset(); tick(); }, 3200);
      return;
    }
    const call = calls[step];

    call.classList.add('run');
    timer = setTimeout(() => {
      call.classList.replace('run', 'done');
      effects[step]();
      step++;
      undo.textContent = `${step} undo step${step === 1 ? '' : 's'}`;
      timer = setTimeout(tick, 380);
    }, 620);
  };

  if (REDUCED) {
    calls.forEach((c, i) => { c.classList.add('done'); effects[i](); });
    undo.textContent = `${calls.length} undo steps`;
    return;
  }

  new IntersectionObserver(([e]) => {
    if (e.isIntersecting && !timer) tick();
    if (!e.isIntersecting && timer) { clearTimeout(timer); timer = null; reset(); }
  }, { threshold: 0.35 }).observe(document.getElementById('live-demo'));
};

const keyLabels = () => {
  const mac = OS === 'macos';
  const names = mac ? { mod: '⌘', alt: '⌥', shift: '⇧' } : { mod: 'Ctrl', alt: 'Alt', shift: 'Shift' };

  document.querySelectorAll('kbd[data-keys]').forEach(kbd => {
    kbd.innerHTML = kbd.dataset.keys.split(' ').map(k => `<b>${names[k] || k}</b>`).join('');
  });
};

const activeNav = () => {
  const links = [...document.querySelectorAll('.nav-links a[href^="#"]')];
  const spy = new IntersectionObserver(entries => {
    entries.forEach(e => {
      if (!e.isIntersecting) return;
      links.forEach(a => a.classList.toggle('active', a.getAttribute('href') === `#${e.target.id}`));
    });
  }, { rootMargin: '-45% 0px -50% 0px' });

  links.forEach(a => {
    const target = document.querySelector(a.getAttribute('href'));

    if (target) spy.observe(target);
  });
};

/* Boot */

document.documentElement.classList.add('js');
splitHeadline();
keyLabels();
activeNav();
tiltShots();
runDemo();

document.querySelectorAll('.reveal').forEach(el => revealer.observe(el));
document.querySelectorAll('.lit').forEach(light);

document.querySelectorAll('.copy-btn').forEach(btn => {
  btn.addEventListener('click', () => copy(btn.dataset.copy, 'Command'));
});

const belt = document.querySelector('.belt');

const twin = belt.cloneNode(true);

// A second copy makes the loop seamless; screen readers only need the first.
twin.setAttribute('aria-hidden', 'true');
belt.after(twin);

const counter = document.getElementById('tool-count');

new IntersectionObserver(([e], obs) => {
  if (!e.isIntersecting) return;
  if (!REDUCED) countUp(counter);
  obs.disconnect();
}, { threshold: 0.6 }).observe(counter);

addEventListener('scroll', onScroll, { passive: true });
addEventListener('resize', onScroll);
onScroll();

addEventListener('pointermove', e => {
  const spot = document.querySelector('.spot');

  spot.style.setProperty('--mx', `${e.clientX}px`);
  spot.style.setProperty('--my', `${e.clientY}px`);
}, { passive: true });

loadRelease().then(renderRelease).catch(showReleaseError);
