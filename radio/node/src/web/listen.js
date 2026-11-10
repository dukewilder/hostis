const $=id=>document.getElementById(id), a=$('a');
let stations=[], cur=null, playing=false;
function bits(id){const b=[];for(const c of id){const v=parseInt(c,16);for(let i=3;i>=0;i--)b.push((v>>i)&1)}return b}
function flag(el,id,live){const b=bits(id);let h='';for(let r=0;r<6;r++)for(let c=0;c<9;c++){const half=c<5?c:8-c;const on=b[(r*5+half)%b.length];h+=on?'<b>1</b>':'<span>0</span>'}el.innerHTML=h;el.classList.toggle('live',!!live)}
function show(s){cur=s;$('embed').textContent='<audio controls src="'+location.origin+'/listen/'+s.id+'.mp3"></audio>';flag($('flag'),s.id,s.live);art(s.cover);$('banner').style.backgroundImage=s.banner?'url(/art/'+s.banner+'.jpg)':'';$('name').textContent=s.name;$('cs').textContent=s.call_sign+' · '+s.aboard+' aboard';$('air').innerHTML=s.live?'<span class="air">ON AIR</span>':'';$('go').hidden=false;$('go').textContent=playing?'■ STOP':'▶ LISTEN';document.querySelectorAll('.row').forEach(r=>r.classList.toggle('on',r.dataset.id===s.id));now()}
function art(h){const c=$('cover');if(h){if(c.dataset.h!==h){c.src='/art/'+h+'.jpg';c.dataset.h=h}c.hidden=false;$('flag').hidden=true}else{c.hidden=true;$('flag').hidden=false}}
// A stream that ends or breaks while the listener still wants it (the tower came down over
// dead air, the network dropped) is asked for again, a little later each time.
let retry=null,tries=0;
function tune(s){clearTimeout(retry);show(s);a.src='/listen/'+s.id+'.mp3?'+Date.now();a.play().then(()=>{playing=true;tries=0;$('go').textContent='■\uFE0E STOP'}).catch(()=>{if(!playing)$('go').textContent='▶\uFE0E LISTEN'})}
function again(){if(!playing||!cur)return;clearTimeout(retry);$('np').textContent='Reconnecting';retry=setTimeout(()=>playing&&tune(cur),Math.min(30000,2000*2**tries++))}
a.addEventListener('ended',again);a.addEventListener('error',()=>a.getAttribute('src')&&again());
$('go').onclick=()=>{if(playing){clearTimeout(retry);playing=false;a.pause();a.removeAttribute('src');a.load();$('go').textContent='▶\uFE0E LISTEN'}else if(cur){playing=true;tune(cur)}};
async function now(){if(!cur)return;try{const n=await (await fetch('/now/'+cur.id)).json();$('np').textContent=n.live?'Live':(n.title||'');art(n.art||cur.cover);$('by').textContent=n.by||''}catch(e){}}
async function log(){try{stations=await (await fetch('/log.json')).json()}catch(e){return}
const rows=$('rows');rows.innerHTML='';for(const s of stations){const r=document.createElement('div');r.className='row';r.dataset.id=s.id;r.innerHTML='<div class="mini"></div><div class="meta"><div class="name"></div><div class="cs"></div><div class="about"></div></div><div class="count"></div>';if(s.cover){r.querySelector('.mini').classList.add('pic');r.querySelector('.mini').innerHTML='<img alt="" src="/art/'+s.cover+'.jpg">'}else flag(r.querySelector('.mini'),s.id,s.live);r.querySelector('.name').textContent=s.name+(s.live?'  · ON AIR':'');r.querySelector('.cs').textContent=s.call_sign;r.querySelector('.about').textContent=s.about||'';r.querySelector('.count').innerHTML=s.aboard+'<small>ABOARD</small>';r.onclick=()=>tune(s);rows.appendChild(r)}
if(!cur&&stations.length)show(stations[0]);else if(cur){const u=stations.find(x=>x.id===cur.id);if(u){cur=u;$('cs').textContent=u.call_sign+' · '+u.aboard+' aboard'}}}
// A station link (#<id>) tunes here if this Lantern carries the station, and offers the app if not.
function followLink(){const id=(location.hash||'').slice(1);if(!/^[0-9a-f]{64}$/.test(id))return;const s=stations.find(x=>x.id===id);const box=$('elsewhere');if(s){box.hidden=true;show(s);return}
box.innerHTML='Someone passed you a station that plays in the app.<br><br><a href="intent://station/'+id+'#Intent;scheme=pyrate;package=black.hostis.pyrate;S.browser_fallback_url='+encodeURIComponent('https://hostis.black/pyrateradio/')+';end">OPEN IN PYRATE RADIO</a> &nbsp;·&nbsp; <a href="https://hostis.black/pyrateradio/">TAKE THE SET</a>';box.hidden=false}
log().then(followLink);window.addEventListener('hashchange',followLink);setInterval(log,15000);setInterval(now,5000);
