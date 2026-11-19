const { chromium } = require('playwright');
const { fixtures, mockBridge } = require('../tests/ui.cjs');
const fs = require('node:fs');
const assert = require('node:assert/strict');
(async () => {
 const browser = await chromium.launch({ executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless:true });
 try {
  const data=fixtures('full'); data.list_clips=data.list_clips.slice(0,3);
  for(const c of data.list_clips)c.created_ms=Date.now()-1000;
  if(data.screenshots[0])data.screenshots[0].modified_ms=Date.now()-1000;
  const page=await browser.newPage({viewport:{width:1280,height:800}});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(mockBridge,data);
  const jpg=fs.readFileSync('D:/reruns/app/tests/local-assets/thumb-0.jpg');
  const video=fs.readFileSync('D:/reruns/app/tests/local-assets/clip.webm');
  await page.route('http://127.0.0.1:2/**',r=>r.fulfill({contentType:'image/jpeg',body:jpg,headers:{'Access-Control-Allow-Origin':'*'}}));
  await page.route('http://127.0.0.1:1/**',r=>r.fulfill({contentType:r.request().url().endsWith('?thumb')?'image/jpeg':'video/webm',body:r.request().url().endsWith('?thumb')?jpg:video,headers:{'Access-Control-Allow-Origin':'*'}}));
  await page.goto('http://localhost:5173/');await page.locator('.card-caption').first().waitFor();
  async function geometry(label){
   const cards=await page.locator('.media-card').evaluateAll(cards=>cards.map(c=>{const r=c.getBoundingClientRect(),cap=c.querySelector('.card-caption'),cb=cap.getBoundingClientRect(),copy=c.querySelector('.clip-copy'),cp=copy?.getBoundingClientRect(),id=c.querySelector('.caption-identity').getBoundingClientRect(),det=c.querySelector('.card-details');return {height:r.height,caption:cb.height,width:r.width,copyWidth:cp?.width,identityRight:id.right,copyLeft:cp?.left,detailsOverflow:det?det.scrollWidth>det.clientWidth:false};}));
   for(const c of cards){assert.equal(c.caption,40);assert.ok(c.identityRight<=c.copyLeft);assert.equal(c.detailsOverflow,false);}
   console.log(label,JSON.stringify(cards));
  }
  await geometry('1280 medium');
  await page.screenshot({path:'D:/reruns/app/tests/shots/cards-shell-1280.png'});
  await page.setViewportSize({width:900,height:600});
  await page.getByRole('button',{name:'View options',exact:true}).click();await page.getByRole('menuitemradio',{name:'Small thumbnails',exact:true}).click();
  await page.waitForTimeout(150);await geometry('900 small');
  await page.getByRole('button',{name:'View options',exact:true}).click();await page.getByRole('menuitemradio',{name:'Largest first',exact:true}).click();
  await page.waitForTimeout(100);await geometry('900 dated');
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.getByRole('navigation',{name:'Settings pages'}).getByRole('button',{name:'Sharing',exact:true}).click();
  assert.ok(await page.getByRole('heading',{name:'Sharing',exact:true}).isVisible());
  await page.getByRole('button',{name:'Back to clips',exact:true}).click();
  await page.getByRole('button',{name:'View options',exact:true}).click();await page.getByRole('menuitemradio',{name:'Newest first',exact:true}).click();
  await page.getByRole('button',{name:'Filter by game: All games',exact:true}).click();
  await page.getByRole('menuitemradio',{name:/Elden Ring/i}).click();
  await page.locator('.screenshot-card').first().waitFor();
  await geometry('900 mixed clips and screenshots');
  await page.evaluate(()=>window.__copyError='Could not copy C:/Users/Player/Videos/'.repeat(45));
  await page.locator('.media-card[data-clip-path$="/clip-2.mp4"]').getByRole('button',{name:'Copy clip',exact:true}).click();
  await page.getByRole('button',{name:'Try again',exact:true}).waitFor();
  console.log('long error',await page.getByRole('button',{name:'Try again',exact:true}).boundingBox());
  assert.ok((await page.getByRole('button',{name:'Try again',exact:true}).boundingBox()).y<550);
  await page.screenshot({path:'D:/reruns/app/tests/shots/cards-shell-error-900.png'});
  await page.getByRole('button',{name:'Dismiss notification',exact:true}).click();
  await page.evaluate(()=>window.__copyError=null);
  const saved=data.list_clips[2].path;
  async function fireSaved(){await page.evaluate(path=>window[`_${window.__listeners['clip-saved']}`]({event:'clip-saved',id:1,payload:path}),saved);}
  await fireSaved();
  const notice=page.getByRole('region',{name:'Notification',exact:true});
  await notice.getByRole('button',{name:'Open',exact:true}).focus();
  await page.waitForTimeout(8200);
  assert.ok(await notice.getByRole('button',{name:'Open',exact:true}).isVisible());
  await notice.getByRole('button',{name:'Copy',exact:true}).click();
  await page.waitForFunction(path=>window.__copied===path,saved);
  console.log('saved notice focus paused and copy source stable');
  await fireSaved();
  await notice.getByRole('button',{name:'Open',exact:true}).click();
  await page.getByRole('button',{name:'Back to the library',exact:true}).click();
  console.log('return focus after notice Open',await page.evaluate(()=>({tag:document.activeElement.tagName,cls:document.activeElement.className,label:document.activeElement.getAttribute('aria-label')})));
  const deleteCard=page.locator('.media-card').filter({has:page.locator('.clip-open')}).first();
  const deletedPath=await deleteCard.getAttribute('data-clip-path');
  await deleteCard.getByRole('button',{name:'Clip actions',exact:true}).click();
  await page.getByRole('menuitem',{name:'Delete',exact:true}).click();
  await notice.getByRole('button',{name:'Undo',exact:true}).waitFor();
  await fireSaved();
  assert.ok(await notice.getByRole('button',{name:'Undo',exact:true}).isVisible());
  assert.equal(await notice.getByRole('button',{name:'Copy',exact:true}).count(),0);
  await notice.getByRole('button',{name:'Undo',exact:true}).click();
  await notice.getByRole('button',{name:'Copy',exact:true}).waitFor();
  assert.ok(await page.locator(`.media-card[data-clip-path="${deletedPath}"]`).isVisible());
  console.log('save during Undo preserves Undo; Undo restores clip then shows queued saved result');
  console.log('browser errors',errors);assert.deepEqual(errors,[]);
 } finally {await browser.close();}
})();
