const { chromium } = require('playwright');
const fs = require('node:fs');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true });
  try {
    const errors = [];
    const page = await browser.newPage({ viewport: { width: 1080, height: 1000 }, deviceScaleFactor: 1 });
    // The real inline host supplies Lucide. Use the installed official icon
    // definitions for this offline wrapper's equivalent, without network calls.
    const definitions = {};
    for (const name of ['copy','play','x','check','minus','gallery-vertical-end','star','settings-2','chevron-down','chevron-left','chevron-right','search','sliders-horizontal','arrow-left']) {
      const file = fs.readFileSync(`D:/reruns/app/node_modules/@lucide/svelte/dist/icons/${name}.svelte`, 'utf8');
      definitions[name] = JSON.parse(file.match(/export const __iconData = (.*?);/s)[1]);
    }
    await page.route('https://unpkg.com/lucide@*/dist/umd/lucide.js', route => route.fulfill({ contentType: 'application/javascript', body: `const rrIcons=${JSON.stringify(definitions)}; globalThis.lucide={createIcons(){document.querySelectorAll('[data-lucide]').forEach(el=>{const d=rrIcons[el.dataset.lucide];if(!d)return;const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');Object.entries({viewBox:'0 0 24 24',fill:'none',stroke:'currentColor','stroke-width':'1.75','stroke-linecap':'round','stroke-linejoin':'round',width:'17',height:'17','aria-hidden':'true'}).forEach(([k,v])=>svg.setAttribute(k,v));d.node.forEach(([tag,attrs])=>{const node=document.createElementNS(svg.namespaceURI,tag);Object.entries(attrs).forEach(([k,v])=>node.setAttribute(k,v));svg.append(node)});el.replaceWith(svg)})}};` }));
    page.on('pageerror', e => errors.push(e.message));
    await page.goto('file:///D:/reruns/app/tests/shots/product-direction-preview.html');
    const frame = page.frames().find(f => f.parentFrame());
    await frame.locator('#rr-direction .rr-clip').first().waitFor();
    await page.waitForTimeout(1600);
    for (const [variant, file] of [['Game shelf', 'shelf'], ['Game dock', 'dock'], ['Install & notifications', 'system']]) {
      await frame.evaluate(name => document.querySelectorAll('#rr-direction [data-variant]').forEach(el => el.hidden = el.dataset.variant !== name), variant);
      const section = frame.locator(`[data-variant="${variant}"]`);
      await section.screenshot({ path: `D:/reruns/app/tests/shots/product-${file}-1024.png` });
    }
    await frame.evaluate(() => document.querySelectorAll('#rr-direction [data-variant]').forEach(el => el.hidden = el.dataset.variant !== 'Game shelf'));
    const app = frame.locator('.rr-shelf .rr-app');
    await app.getByRole('button', { name: 'Save 30s', exact: false }).click();
    await app.getByRole('region', { name: 'Clip saved', exact: true }).waitFor();
    await page.waitForTimeout(220);
    await app.screenshot({ path: 'D:/reruns/app/tests/shots/product-save-1024.png' });
    await app.locator('[data-popup-copy]').click();
    await app.getByText('Copied to clipboard', { exact: true }).waitFor();
    await app.getByRole('button', { name: 'Dismiss notification' }).click();
    await app.getByRole('button', { name: 'Deadlock', exact: true }).click();
    assert.equal(await app.locator('.rr-clip').count(), 1);
    await app.getByRole('button', { name: 'All clips', exact: true }).click();
    await app.getByRole('textbox', { name: 'Search clips' }).fill('Overwatch');
    assert.equal(await app.locator('.rr-clip').count(), 1);
    await app.getByRole('textbox', { name: 'Search clips' }).fill('');
    for (const width of [736, 360, 320]) {
      await page.setViewportSize({ width, height: 1300 });
      await page.waitForTimeout(150);
      const geometry = await frame.evaluate(() => {
        const root = document.getElementById('rr-direction');
        const app = root.querySelector('.rr-shelf .rr-app');
        return { root: root.clientWidth, app: app.clientWidth, scroll: app.scrollWidth, images: [...app.querySelectorAll('img')].every(i => i.complete && i.naturalWidth > 0), icons: app.querySelectorAll('svg').length };
      });
      assert.ok(geometry.scroll <= geometry.app + 1, `Overflow at ${width}: ${JSON.stringify(geometry)}`);
      assert.ok(geometry.images, 'Images loaded');
      assert.ok(geometry.icons > 10, 'Official icons loaded');
      await app.screenshot({ path: `D:/reruns/app/tests/shots/product-shelf-${width}.png` });
      console.log(width, geometry);
    }
    assert.deepEqual(errors, []);
    console.log('Product concept interactions and responsive checks passed');
  } finally { await browser.close(); }
})();
