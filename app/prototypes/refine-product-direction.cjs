const fs = require('node:fs');
const file = 'C:/Users/Lucas Pearson/.codex/visualizations/2026/11/15/01a1e105-50d6-7be3-8b66-edf877fd747b/reruns-product-direction.html';
let source = fs.readFileSync(file, 'utf8');
source = source.replaceAll('That one\'s a keeper.', 'Clip saved')
  .replaceAll('Copied. Send it.', 'Copied to clipboard')
  .replace("{id:'fortnite',name:'Fortnite',count:7}", "{id:'fortnite',name:'Fortnite',count:2}")
  .replace("{id:'overwatch',name:'Overwatch 2',count:2}", "{id:'overwatch',name:'Overwatch 2',count:1}")
  .replace("{id:'deadlock',name:'Deadlock',count:2}", "{id:'deadlock',name:'Deadlock',count:1}")
  .replaceAll('<small>14</small>', '<small>6</small>')
  .replaceAll('<span class="rr-total">14</span>', '<span class="rr-total">6</span>')
  .replace("scope==='all'&&!search?'14'", "scope==='all'&&!search?'6'")
  .replace('${g.count} clips', "${g.count} ${g.count===1?'clip':'clips'}")
  .replace('    paint();\n  }\n  root.querySelectorAll(\'.rr-app\').forEach(build);', `    paint();
    return { layout, restore(next) { if (['all','favorites',...games.map(g=>g.id)].includes(next)) {scope=next;paint();} } };
  }
  const controllers=[...root.querySelectorAll('.rr-app')].map(build);
  window.addEventListener('openai:set_globals',event=>{
    const next=event.detail?.globals?.widgetState?.modelContent;
    if(next) controllers.find(c=>c.layout===next.direction)?.restore(next.scope);
  });`);
fs.writeFileSync(file, source);
