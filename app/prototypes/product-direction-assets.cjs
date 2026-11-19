const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');
const source = 'D:/reruns/app/tests/local-assets';
const steam = 'C:/Program Files (x86)/Steam/appcache/librarycache';
const output = 'C:/Users/Lucas Pearson/.codex/visualizations/2026/11/15/01a1e105-50d6-7be3-8b66-edf877fd747b/reruns-product-direction.html';
function find(dir, name) {
  for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
    const filename = path.join(dir, item.name);
    if (item.isDirectory()) { const result = find(filename, name); if (result) return result; }
    else if (item.name === name) return filename;
  }
}
(async () => {
  const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true });
  try {
    const page = await browser.newPage();
    async function picture(filename, width) {
      const src = 'data:image/jpeg;base64,' + fs.readFileSync(filename).toString('base64');
      return page.evaluate(async ({ src, width }) => {
        const image = new Image(); image.src = src; await image.decode();
        const scale = Math.min(1, width / image.width);
        const canvas = document.createElement('canvas'); canvas.width = Math.round(image.width * scale); canvas.height = Math.round(image.height * scale);
        canvas.getContext('2d').drawImage(image, 0, 0, canvas.width, canvas.height);
        return canvas.toDataURL('image/jpeg', 0.88);
      }, { src, width });
    }
    const assets = {};
    for (const i of [0, 1, 8, 3, 17, 9]) assets['thumb' + i] = await picture(path.join(source, `thumb-${i}.jpg`), 560);
    for (const [game, appid] of [['siege', '359550'], ['overwatch', '2357570'], ['deadlock', '1422450']]) assets[game] = await picture(find(path.join(steam, appid), 'library_capsule.jpg'), 120);
    assets.fortnite = await picture(path.join(source, 'thumb-8.jpg'), 120);
    let content = fs.readFileSync(output, 'utf8');
    if (!content.includes('/*RR_ASSETS*/{}')) throw new Error('Asset placeholder missing');
    content = content.replace('/*RR_ASSETS*/{}', JSON.stringify(assets));
    if (Buffer.byteLength(content) > 1000000) throw new Error('Fragment too large');
    fs.writeFileSync(output, content);
    console.log(`Wrote ${Buffer.byteLength(content).toLocaleString()} bytes`);
  } finally { await browser.close(); }
})();
