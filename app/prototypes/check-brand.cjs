const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');
const root = path.resolve(__dirname, '..');
const source = p => fs.readFileSync(path.join(root, p));
const svg = p => `data:image/svg+xml;base64,${source(p).toString('base64')}`;
const png = p => `data:image/png;base64,${source(p).toString('base64')}`;
(async () => {
  const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 920, height: 730 }, deviceScaleFactor: 1 });
    await page.setContent(`<style>*{box-sizing:border-box}body{margin:0;background:#202123;color:#f1f0eb;font:13px Segoe UI;padding:32px}h1{font-size:16px;font-weight:500;margin:0 0 24px}section{display:flex;align-items:center;gap:32px;margin:20px 0 28px}small{width:110px;color:#a6a7ab}img{display:block;object-fit:contain}.onlight{background:#e6e6e3;color:#222;padding:20px;margin:0 -32px}.cell{display:grid;justify-items:center;gap:10px;min-width:56px}.pixel{image-rendering:pixelated}.lockup{display:flex;gap:12px;align-items:center}</style>
      <h1>Reruns — native size checks</h1>
      <section><small>Approved mark</small>${[16,24,32,48].map(s=>`<div class="cell"><img width="${s}" height="${s}" src="${svg('public/brand-mark.svg')}"><span>${s}px</span></div>`).join('')}</section>
      <section><small>Wordmark</small><img width="130" height="26" src="${svg('public/brand-wordmark.svg')}"><div class="lockup"><img width="28" height="28" src="${svg('public/brand-mark.svg')}"><img width="110" height="22" src="${svg('public/brand-wordmark.svg')}"></div></section>
      <section><small>Desktop icon</small>${[16,24,32,48,64].map(s=>`<div class="cell"><img width="${s}" height="${s}" src="${s===32?png('src-tauri/icons/32x32.png'):svg('src-tauri/icons/icon.svg')}"><span>${s}px</span></div>`).join('')}<img class="pixel" width="128" height="128" src="${png('src-tauri/icons/32x32.png')}"></section>
      <section class="onlight"><small>Light desktop</small>${[16,24,32,48,64].map(s=>`<div class="cell"><img width="${s}" height="${s}" src="${svg('src-tauri/icons/icon.svg')}"><span>${s}px</span></div>`).join('')}</section>
      <section><small>Windows ICO<br>actual frames</small>${[16,24,32].map(s=>`<div class="cell"><img width="${s}" height="${s}" src="${png(`prototypes/brand-generated/ico-${s}.png`)}"><img class="pixel" width="96" height="96" src="${png(`prototypes/brand-generated/ico-${s}.png`)}"><span>${s}px / zoomed</span></div>`).join('')}</section>`);
    await page.locator('img').last().waitFor();
    await page.evaluate(async()=>Promise.all([...document.images].map(i=>i.decode())));
    await page.screenshot({ path: path.join(__dirname, 'brand-preview.png') });
    console.log('Brand preview rendered:',path.join(__dirname,'brand-preview.png'));
  } finally { await browser.close(); }
})();
