const { chromium } = require('playwright');
const references = [
  ['raycast-icons','https://www.raycast.com/uploads/redesign/icons.png'],
  ['raycast-feedback','https://www.raycast.com/uploads/redesign/actionbar-left.png'],
  ['cleanshot-actions','https://cleanshot.com/_ipx/f_png%26q_90%26s_400x270/img/features/quick_actions.png'],
  ['moments-gallery','https://images.squarespace-cdn.com/content/v1/5ea4dbb320b8fa08d5dc37a5/c5fb39c4-5f96-4785-90ff-d892bb453127/Moments2.png'],
];
(async()=>{
  const browser=await chromium.launch({executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',headless:true});
  try {
    const results=await Promise.allSettled(references.map(async([name,url])=>{
      const page=await browser.newPage({viewport:{width:1440,height:1000}});
      try {
        const response=await page.goto(url,{waitUntil:'load',timeout:20000});
        if(!response.ok()) throw Error(`HTTP ${response.status()}`);
        await page.locator('img').evaluate(async el=>{await el.decode();el.style.cssText='max-width:1400px;max-height:960px;object-fit:contain;display:block;margin:20px auto;';document.body.style.margin='0';});
        await page.screenshot({path:`D:/reruns/app/tests/shots/research-${name}-source.png`});
        return {name,url};
      } finally {await page.close();}
    }));
    results.forEach((r,i)=>console.log(references[i][0],r.status,r.status==='rejected'?String(r.reason):'visually available'));
  } finally {await browser.close();}
})();
