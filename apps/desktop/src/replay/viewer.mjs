const english = {
  "ที่ตายก่อนนาที 15 อยู่รอบเลนกลาง": "deaths before 15:00 occurred near mid",
  "เกม": "Match",
  "แพตช์เกม": "Game patch",
  "ยังไม่ทราบแพตช์": "Patch unknown",
  "ไม่มีแมตช์ในแพตช์นี้": "No matches for this patch.",
  "โหลดภาพแผนที่ไม่สำเร็จ": "The map image could not be loaded.",
  "ทุกเกม": "All matches",
  "เลือกเอง": "Custom",
  "เลือกหลายเกม": "Select matches",
  "ช่วงเวลา": "Time range",
  "ทั้งเกม": "Full match",
  "ก่อนนาที 10": "Before 10:00",
  "นาที 10 ถึงก่อน 15": "10:00 to 14:59",
  "ตั้งแต่นาที 15": "From 15:00",
  "ชั้นข้อมูล": "Map layer",
  "ฮีตแมป + จุดตาย": "Heatmap and deaths",
  "ฮีตแมป": "Heatmap",
  "จุดตาย": "Death",
  "รีเพลย์เส้นทาง": "Replay paths",
  "เลือกเกมที่ต้องการเทียบ": "Choose matches to compare",
  "ใช้เกมที่เลือก": "Use selected matches",
  "เล่น": "Play",
  "หยุด": "Pause",
  "ความเร็วรีเพลย์": "Playback speed",
  "เวลาในรีเพลย์": "Replay time",
  "ฮีตแมปตำแหน่งและจุดตาย กดหมุดหรือเลือกจากรายการเพื่อดูรายละเอียด": "Position heatmap and death pins. Select a pin or an event to see details.",
  "อยู่บ่อยขึ้นตามตัวกรอง": "Higher position density",
  "รอบเลนกลาง": "Mid area",
  "เส้นทาง 45 วินาทีล่าสุด · สีตามรายชื่อฮีโร่": "Last 45 seconds · colors match the roster",
  "ฮีโร่และจังหวะตาย": "Heroes and death events",
  "เลือกฮีโร่บนแผนที่": "Choose heroes on the map",
  "เลือกทีม": "Choose a team",
  "ดูรายตัว": "Solo hero",
  "วินาทีที่ฮีโร่มีชีวิต": "Selected hero-seconds",
  "ครั้งที่เราตาย": "Your deaths",
  "ตายรอบเลนกลาง": "Deaths near mid",
  "จังหวะตาย": "Death events",
  "วิธีอ่านฮีตแมป": "How to read the heatmap",
  "ฮีตแมปใช้ตำแหน่งฮีโร่จาก DEM หนึ่งตัวอย่างต่อวินาทีขณะมีชีวิต": "The heatmap uses one recorded DEM position per second while a hero is alive.",
  "หมุดแสดงพิกัด ณ tick ที่ตาย วงรอบเลนกลางใช้รัศมี 2,200 หน่วย": "Death pins use the exact recorded death tick. The mid area has a 2,200-unit radius.",
  "เลือกหลายเกมในแพตช์เดียวกันเพื่อเทียบจากเวลา 0:00 กดจุดตายเพื่อดู 15 วินาทีก่อนหน้า": "Compare matches from the same patch at 0:00. Select a death to replay the previous 15 seconds.",
  " จาก ": " of ",
  " ครั้ง": "",
  "ผู้สังหาร ": "Killed by ",
  "จังหวะปิดคิล": "Finishing hit",
  "โจมตีปกติ": "Basic attack",
  "ไม่ระบุสกิล": "Ability not recorded",
  "ไม่ทราบผู้โจมตี": "Unknown attacker",
  "ร่างปลอมของ ": "Illusion credited to ",
  "สิ่งที่ลองทำได้": "What to try",
  "เหตุการณ์จาก Combat Log": "Combat Log events",
  "ไม่มีรายละเอียด Combat Log สำหรับจังหวะนี้": "Combat Log details are unavailable for this death.",
  "ใช้": "cast",
  "คุมด้วย": "controlled with",
  "ทำดาเมจด้วย": "hit with",
  "น้ำพุ": "Fountain",
  "เลือกจุดตายเพื่อเล่น 15 วินาทีก่อนตาย": "Select a death to replay the previous 15 seconds.",
  " · จบ ": " · ended ",
  "ไม่มีจุดตายในช่วงที่เลือก": "No deaths in the selected range.",
  "ทั้งหมด": "All",
  "เรา": "You",
  "ทีมเรา": "Your team",
  "ศัตรู": "Enemies",
  "เลือกฮีโร่": "Select hero",
  " (เรา)": " (you)",
  " เกม": " matches",
  "ก่อนตาย ": "Before death in ",
  " วินาที": "s",
  "จบเกม": "Match ended",
  "เลือกฮีโร่เพื่อดูเส้นทาง": "Select a hero to see a route",
  "ฮีโร่บนแผนที่ ": "Heroes on map ",
  " ตัว": "",
  " · เทียบ ": " · comparing ",
  " เกมจาก 0:00": " matches from 0:00",
  "จุดตายของเรา": "Your death",
  "กลับฮีตแมป": "Back to heatmap",
  "ใช้ ": "Use ",
  "ปิดตัวเลือกเกม": "Close match selector",
  "เลือกเอง · ": "Custom · "
};

export function mountReplay(root, report, { locale, markup, style }) {
  const controller = new AbortController();
  const { signal } = controller;
  const t = text => locale === "en" ? english[text] ?? text : text;
  const template = document.createElement("template");
  template.innerHTML = markup;
  for (const element of template.content.querySelectorAll("[aria-label]")) {
    element.setAttribute("aria-label", t(element.getAttribute("aria-label")));
  }
  const walk = document.createTreeWalker(template.content, NodeFilter.SHOW_TEXT);
  for (let node = walk.nextNode(); node; node = walk.nextNode()) {
    const key = node.nodeValue.trim();
    if (key) node.nodeValue = node.nodeValue.replace(key, t(key));
  }
  const styles = document.createElement("style");
  styles.textContent = style;
  root.replaceChildren(styles, template.content);
  let matches = [];
  let layout;
  let calibration;
  const el = id => root.getElementById(id);
  const map = el("map");
  const canvas = map.getContext("2d");
  function clock(sec, exact=false){
    const absolute=Math.abs(sec);
    const seconds=exact ? (absolute%60).toFixed(3).padStart(6,"0") : String(Math.floor(absolute)%60).padStart(2,"0");
    return (sec<0 ? "−" : "")+Math.floor(absolute/60)+":"+seconds;
  }
  const projected = (x,y) => [calibration.ox+x*calibration.sx,calibration.oy+y*calibration.sy];
  const positionText = (x,y) => [x,y].map(n=>n.toLocaleString("en-US",{maximumFractionDigits:3})).join(", ");
  const inMid = (x,y) => Math.hypot(x,y)<=2200;
  const phaseCheck = {all:()=>true,early:t=>t<600,rotation:t=>t>=600&&t<900,late:t=>t>=900};
  const view = {deaths:[],key:null};
  let activeReview=null;
  const patchLabel = patch => patch ?? t("ยังไม่ทราบแพตช์");
  let heatLayer;
  function heatmap(points) {
    const w = Math.ceil(map.width/4), h = Math.ceil(map.height/4);
    const layer = document.createElement("canvas");
    layer.width = w;
    layer.height = h;
    const context = layer.getContext("2d");
    const cells = new Float32Array(w*h);
    for (const [,x,y] of points) {
      const [u,v] = projected(x,y);
      const px = Math.round(u/4), py = Math.round(v/4);
      if (px>=0 && px<w && py>=0 && py<h) cells[py*w+px]++;
    }
    const radius = 11, sigma = 3.25;
    const kernel = Array.from({length:2*radius+1},(_,i) => Math.exp(-.5*((i-radius)/sigma)**2));
    const xpass = new Float32Array(cells.length), smoothed = new Float32Array(cells.length);
    for (let y=0;y<h;y++) {
      for (let x=0;x<w;x++) {
        let sum = 0;
        for (let k=Math.max(-x,-radius);k<=Math.min(w-x-1,radius);k++) sum += cells[y*w+x+k]*kernel[k+radius];
        xpass[y*w+x] = sum;
      }
    }
    for (let y=0;y<h;y++) {
      for (let x=0;x<w;x++) {
        let sum = 0;
        for (let k=Math.max(-y,-radius);k<=Math.min(h-y-1,radius);k++) sum += xpass[(y+k)*w+x]*kernel[k+radius];
        smoothed[y*w+x] = sum;
      }
    }
    const positives = Array.from(smoothed).filter(v=>v > .10).sort((a,b)=>a-b);
    const cap = Math.max(1, positives[Math.floor(positives.length*.98)] || 1);
    const pixels = context.createImageData(w,h);
    for (let i=0;i<smoothed.length;i++) {
      const f = Math.min(1,Math.log1p(smoothed[i]*.7)/Math.log1p(cap*.7));
      if (f < .04) continue;
      const intensity = Math.max(0,(f-.52)/.48), j=i*4;
      pixels.data[j] = 44 + 194*intensity;
      pixels.data[j+1] = 127 + 119*f;
      pixels.data[j+2] = 157 - 35*intensity;
      pixels.data[j+3] = 220*f;
    }
    context.putImageData(pixels,0,0);
    return layer;
  }

  const mapBase=document.createElement("canvas");
  mapBase.width=map.width;
  mapBase.height=map.height;
  function loadBasemap(background){
    const surface=mapBase.getContext("2d");
    surface.fillStyle="#182123";
    surface.fillRect(0,0,map.width,map.height);
    if(background) surface.drawImage(background,0,0,map.width,map.height);
    surface.fillStyle="rgba(9,18,23,.55)";
    surface.fillRect(0,0,map.width,map.height);
  }

  function draw(){
    const layer=el("layer").value;
    canvas.drawImage(mapBase,0,0);
    const active=Boolean(replay.match);
    const tracks=active ? selectedReplayTracks() : [];
    const positions=[];
    if(active){
      for(const {actor,marks} of tracks){
        const index=markIndex(replay.time,marks);
        const position=currentPosition(index,marks);
        drawRoute(index,position,marks,actor.color);
        if(position) positions.push({actor,position});
      }
    }else{
      if(layer!=="deaths"&&heatLayer) canvas.drawImage(heatLayer,0,0,map.width,map.height);
      const [cx,cy]=projected(0,0);
      canvas.save();
      canvas.beginPath();
      canvas.ellipse(cx,cy,2200*calibration.sx,-2200*calibration.sy,0,0,Math.PI*2);
      canvas.setLineDash([5,6]);
      canvas.strokeStyle="#fff2b798";
      canvas.lineWidth=1.7;
      canvas.stroke();
      canvas.restore();
      if(layer==="heat") return;
    }
    const visible=active ? view.deaths.filter(row=>row.d.t<=replay.time) : view.deaths;
    const selected=visible.find(row=>row.key===view.key);
    const ordered=visible.filter(row=>row!==selected);
    if(selected) ordered.push(selected);
    for(const row of ordered){
      const [x,y]=projected(row.d.x,row.d.y);
      canvas.beginPath();
      canvas.arc(x,y,row===selected ? 10 : 7.3,0,Math.PI*2);
      canvas.fillStyle=row===selected ? "#fff3af" : "#f46952";
      canvas.strokeStyle="#ffe5cc";
      canvas.lineWidth=row===selected ? 3 : 2.2;
      canvas.fill();
      canvas.stroke();
    }
    for(const {actor,position} of positions) drawHero(position,actor);
  }

  function describeAction(action,kind){
    const actor=(action.illusion ? t("ร่างปลอมของ ") : "")+t(action.actor);
    const move=t(action.ability ?? "โจมตีปกติ");
    return actor+(kind && action.ability ? " "+t(kind)+" " : " · ")+move;
  }

  function showDeathReview(detail,review){
    const finish=document.createElement("p");
    finish.className="review-finish";
    finish.textContent=t("จังหวะปิดคิล")+" · "+describeAction(review.finish);
    detail.append(finish);
    if(review.summary){
      const summary=document.createElement("p");
      summary.className="review-summary";
      summary.textContent=review.summary[locale];
      detail.append(summary);
    }
    if(review.advice){
      const advice=document.createElement("div");
      advice.className="review-advice";
      const heading=document.createElement("strong");
      heading.textContent=t("สิ่งที่ลองทำได้");
      const message=document.createElement("p");
      message.textContent=review.advice[locale];
      advice.append(heading,message);
      detail.append(advice);
    }
    const kindText={cast:"ใช้",control:"คุมด้วย",damage:"ทำดาเมจด้วย"};
    const labels=review.moments.map(moment=>describeAction(moment,kindText[moment.kind]));
    const list=document.createElement("ol");
    list.className="review-moments";
    list.setAttribute("aria-label",t("เหตุการณ์จาก Combat Log"));
    const rows=review.moments.map((moment,index)=>{
      const entry=document.createElement("li");
      const stamp=document.createElement("time");
      stamp.textContent=clock(moment.t);
      const action=document.createElement("span");
      action.textContent=labels[index];
      entry.append(stamp,action);
      list.append(entry);
      return entry;
    });
    if(rows.length){
      const caption=document.createElement("p");
      caption.textContent=t("เหตุการณ์จาก Combat Log");
      detail.append(caption,list);
    }
    return {review,labels,rows,index:null};
  }

  function updateCombatFocus(){
    const compact=el("replay-combat");
    compact.hidden=!activeReview;
    if(!activeReview) return;
    const {review,labels,rows,index:previous,end}=activeReview;
    const index=replay.time>=end ? rows.length : review.moments.findLastIndex(moment=>moment.t<=replay.time);
    if(index===previous) return;
    rows[previous]?.removeAttribute("aria-current");
    rows[index]?.setAttribute("aria-current","step");
    compact.textContent=index<0 ? t("เหตุการณ์จาก Combat Log") : index===rows.length
      ? t("จังหวะปิดคิล")+" · "+describeAction(review.finish) : labels[index];
    activeReview.index=index;
  }

  function choose(key){
    view.key=key;
    const row=view.deaths.find(item=>item.key===key);
    const detail=el("detail");
    detail.replaceChildren();
    activeReview=null;
    if(row){
      const {m,d}=row;
      const title=document.createElement("strong");
      title.textContent=m.hero+" · "+clock(d.t,true)+" · "+patchLabel(m.patch);
      detail.append(title);
      if(d.review){
        activeReview={...showDeathReview(detail,d.review),end:d.t};
      }else{
        if(d.killer){
          const killer=document.createElement("p");
          killer.textContent=t("ผู้สังหาร ")+(d.killer==="dota_fountain" ? t("น้ำพุ") : d.killer);
          detail.append(killer);
        }
        const missing=document.createElement("p");
        missing.textContent=t("ไม่มีรายละเอียด Combat Log สำหรับจังหวะนี้");
        detail.append(missing);
      }
      const coords=document.createElement("p");
      coords.className="review-coordinates";
      coords.textContent="("+positionText(d.x,d.y)+") · tick "+d.tick;
      detail.append(coords);
    }else{
      const hint=document.createElement("p");
      hint.textContent=t("เลือกจุดตายเพื่อเล่น 15 วินาทีก่อนตาย");
      detail.append(hint);
    }
    for(const button of el("events").querySelectorAll("button")){
      button.setAttribute("aria-pressed",String(button.dataset.key===key));
    }
    updateCombatFocus();
    draw();
  }

  function render(){
    const scope=scopes.get(el("match").value);
    const phase=phaseCheck[replay.match ? "all" : el("phase").value];
    const shown=scope.games;
    const firstFifteen=shown.flatMap(match=>match.deaths).filter(d=>d.t<900);
    const earlyMid=firstFifteen.filter(d=>inMid(d.x,d.y)).length;
    el("early-mid").parentElement.hidden=firstFifteen.length===0;
    el("early-mid").textContent=earlyMid+t(" จาก ")+firstFifteen.length+t(" ครั้ง");
    view.deaths=shown.flatMap(m=>m.deaths.map((d,i)=>({m,d,key:m.id+"-"+(i+1)})).filter(row=>phase(row.d.t)));
    if(!replay.match) drawSelection(scope,phase);
    else updateSelectedCount(scope,replay.selected);
    el("dead").textContent=view.deaths.length;
    el("mid").textContent=view.deaths.filter(({d})=>inMid(d.x,d.y)).length+" / "+view.deaths.length;
    const list=el("events");
    list.replaceChildren();
    for(const match of shown){
      const rows=view.deaths.filter(row=>row.m===match);
      if(!rows.length) continue;
      const group=document.createElement("p");
      group.className="match-group";
      group.textContent=match.hero+" · "+match.id+" · "+patchLabel(match.patch)+t(" · จบ ")+clock(match.duration);
      list.append(group);
      for(const {d,key} of rows){
        const button=document.createElement("button");
        button.type="button";
        button.className="event";
        button.dataset.key=key;
        const time=document.createElement("time");
        time.textContent=clock(d.t);
        const coords=document.createElement("span");
        coords.className="coordinate";
        coords.textContent=positionText(d.x,d.y);
        button.append(time,coords);
        button.addEventListener("click",()=>navigateDeath(key,true));
        list.append(button);
      }
    }
    if(!view.deaths.length){
      const empty=document.createElement("p");
      empty.textContent=t(shown.length ? "ไม่มีจุดตายในช่วงที่เลือก" : "ไม่มีแมตช์ในแพตช์นี้");
      list.append(empty);
    }
    choose(null);
  }

  const rosterColors = Object.freeze({
    self:"#70e8c7",
    "ally-1":"#78b5ff",
    "ally-2":"#73e5ff",
    "ally-3":"#c5e985",
    "ally-4":"#a1aafa",
    "enemy-1":"#ff846e",
    "enemy-2":"#ff9ac9",
    "enemy-3":"#ffce72",
    "enemy-4":"#de9cf2",
    "enemy-5":"#ffae6f",
  });

  function makeScope(games){
    const combined=games.length>1;
    const groups=new Map();
    for(const match of games){
      for(const actor of match.roster){
        const relation=actor.key==="self" ? "self" : actor.team===match.team ? "ally" : "enemy";
        const key=combined ? relation+"-"+actor.hero : actor.key;
        if(!groups.has(key)){
          const order=groups.size;
          const color=combined ? (relation==="self" ? rosterColors.self : "hsl("+((order*137.508)%360)+" 84% 72%)")
            : rosterColors[actor.key];
          groups.set(key,{key,hero:actor.hero,team:actor.team,relation,color,entries:[]});
        }
        groups.get(key).entries.push({match,actor,routeKey:combined ? match.id+"/"+actor.key : actor.key});
      }
    }
    const roster=[...groups.values()];
    if(combined){
      const rank={self:0,ally:1,enemy:2};
      roster.sort((a,b)=>rank[a.relation]-rank[b.relation]||a.hero.localeCompare(b.hero));
    }
    const id=games.map(m=>m.id).join("+");
    return {id, team:combined ? null : games[0]?.team ?? null,
      games, start:Math.min(0,...games.map(m=>m.start)), duration:Math.max(0,...games.map(m=>m.duration)), roster};
  }

  const scopes=new Map();
  const userSelections=new Map();
  function selectionFor(scope,playing){
    const saved=userSelections.get(scope.id);
    if(saved) return saved;
    return new Set(scope.roster.filter(actor=>(playing && scope.games.length===1) || actor.relation==="self").map(a=>a.key));
  }
  function selectedEntries(scope,selection){
    return scope.roster.filter(group=>selection.has(group.key)).flatMap(group=>
      group.entries.map(entry=>({...entry,group})));
  }
  function updateSelectedCount(scope,selection){
    const seconds=selectedEntries(scope,selection).reduce((total,{match,actor})=>
      total+(actor.key==="self" ? match.points.length : actor.points.length),0);
    el("samples").textContent=seconds.toLocaleString("en-US");
  }
  function drawSelection(scope,phase){
    const selection=selectionFor(scope,false);
    renderRoster(scope,selection,selectVisibleHeroes);
    const points=selectedEntries(scope,selection).flatMap(({match,actor})=>
      (actor.key==="self" ? match.points : actor.points).filter(p=>phase(p[0])));
    heatLayer=heatmap(points);
    el("samples").textContent=points.length.toLocaleString("en-US");
  }
  function selectVisibleHeroes(selected){
    const scope=scopes.get(el("match").value);
    userSelections.set(scope.id,selected);
    if(replay.match){
      replay.selected=selected;
      renderRoster(scope,selected,selectVisibleHeroes);
      updateSelectedCount(scope,selected);
      refreshReplay();
    }else{
      drawSelection(scope,phaseCheck[el("phase").value]);
      draw();
    }
  }

  function renderRoster(match,selection,onSelect){
    const panel=el("replay-roster");
    const focused=panel.contains(root.activeElement) ? root.activeElement.dataset.rosterFocus : null;
    const allies=match.roster.filter(actor=>actor.relation!=="enemy");
    const enemies=match.roster.filter(actor=>actor.relation==="enemy");
    const presets=el("replay-roster-presets");
    const solo=el("replay-roster-solo");
    const teamList=el("replay-roster-teams");
    const scroll=teamList.scrollTop;
    teamList.classList.toggle("roster-scroll",match.games.length>1);
    presets.replaceChildren();
    solo.replaceChildren();
    teamList.replaceChildren();

    function makeButton(text,pressed,focus){
      const button=document.createElement("button");
      button.type="button";
      button.textContent=text;
      button.setAttribute("aria-pressed",String(pressed));
      button.dataset.rosterFocus=focus;
      return button;
    }

    for(const [label,actors,key] of [
      [t("ทั้งหมด"),match.roster,"all"],
      [t("เรา"),allies.filter(actor=>actor.relation==="self"),"self"],
      [t("ทีมเรา"),allies,"allies"],
      [t("ศัตรู"),enemies,"enemies"],
    ]){
      const keys=actors.map(actor=>actor.key);
      const button=makeButton(label,keys.length===selection.size&&keys.every(actor=>selection.has(actor)),key);
      button.dataset.rosterPreset=key;
      button.addEventListener("click",()=>onSelect(new Set(keys)));
      presets.append(button);
    }

    const prompt=new Option(t("เลือกฮีโร่"),"");
    prompt.disabled=true;
    solo.append(prompt);
    solo.onchange=()=>onSelect(new Set([solo.value]));

    for(const [actors,label] of [[allies,t("ทีมเรา")],[enemies,t("ศัตรู")]]){
      const team=document.createElement("fieldset");
      team.className="roster-team";
      const legend=document.createElement("legend");
      legend.textContent=label;
      team.append(legend);

      for(const actor of actors){
        const name=actor.hero+(actor.relation==="self" ? t(" (เรา)") : "");
        const count=actor.entries.length;
        const description=count>1 ? name+" · "+count+t(" เกม") : name;
        solo.add(new Option(description+" · "+label,actor.key));
        const button=makeButton("",selection.has(actor.key),actor.key);
        button.className="roster-actor";
        button.dataset.actor=actor.key;
        button.dataset.side=actor.relation;
        button.style.setProperty("--actor-color",actor.color);
        const swatch=document.createElement("span");
        swatch.className="roster-dot";
        swatch.setAttribute("aria-hidden","true");
        const title=document.createElement("span");
        title.textContent=description;
        button.append(swatch,title);
        button.addEventListener("click",()=>{
          const next=new Set(selection);
          if(next.has(actor.key)) next.delete(actor.key);
          else next.add(actor.key);
          onSelect(next);
        });
        team.append(button);
      }
      teamList.append(team);
    }

    teamList.scrollTop=scroll;
    solo.value=selection.size===1 ? selection.values().next().value : "";
    if(focused){
      const control=Array.from(panel.querySelectorAll("[data-roster-focus]")).find(node=>node.dataset.rosterFocus===focused);
      control?.focus({preventScroll:true});
    }
  }

  const replay = {match:null, routes:new Map(), selected:new Set(), time:0, rate:1, frame:0, last:null, stopAt:null, returnMatch:"all"};

  function selectedReplayTracks(){
    if(!replay.match) return [];
    return selectedEntries(replay.match,replay.selected)
      .filter(({match})=>replay.time>=match.start && replay.time<=match.duration)
      .map(({group,routeKey})=>({actor:group,marks:replay.routes.get(routeKey)}));
  }
  const range = el("replay-time");
  const playButton = el("replay-play");
  const toggleButton = el("replay-mode");
  const speedButtons = Array.from(el("replay-speed").querySelectorAll("button"));

  function canConnect(a,b){
    return !a.dead && b.t-a.t<=1.5 && Math.hypot(b.x-a.x,b.y-a.y)<1300;
  }

  function makeRoute(actor,match){
    const points=actor.key==="self" ? match.points : actor.points;
    const marks=points.map(([second,x,y])=>({t:Math.min(second+.5,match.duration),x,y,dead:false}));
    for(const d of actor.lifeEnds) marks.push({t:d.t,x:d.x,y:d.y,dead:true});
    marks.sort((a,b)=>a.t-b.t || Number(a.dead)-Number(b.dead));
    return marks;
  }

  function markIndex(time,marks){
    let low=0,high=marks.length;
    while(low<high){
      const mid=Math.floor((low+high)/2);
      if(marks[mid].t<=time) low=mid+1;
      else high=mid;
    }
    return low-1;
  }

  function currentPosition(index,marks){
    const {time}=replay;
    const previous=marks[index], next=marks[index+1];
    if(!previous) return next && !next.dead && next.t-time<=.5 ? next : null;
    if(previous.dead) return null;
    if(next && canConnect(previous,next)){
      const f=(time-previous.t)/(next.t-previous.t);
      return {x:previous.x+(next.x-previous.x)*f,y:previous.y+(next.y-previous.y)*f};
    }
    return time-previous.t<=1.05 ? previous : null;
  }

  function drawRoute(index,position,marks,color){
    let previous=null;
    canvas.beginPath();
    const first=Math.max(0,markIndex(replay.time-45,marks));
    for(let i=first;i<=index;i++){
      const mark=marks[i];
      const coords=projected(mark.x,mark.y);
      if(previous && canConnect(previous,mark)) canvas.lineTo(...coords);
      else canvas.moveTo(...coords);
      previous=mark;
    }
    if(previous && position && !previous.dead) canvas.lineTo(...projected(position.x,position.y));
    canvas.save();
    canvas.lineCap="round";
    canvas.lineJoin="round";
    canvas.strokeStyle="#0d2023";
    canvas.lineWidth=6;
    canvas.stroke();
    canvas.strokeStyle=color;
    canvas.lineWidth=3;
    canvas.stroke();
    canvas.restore();
  }

  function drawHero(position,actor){
    const [x,y]=projected(position.x,position.y);
    const color=actor.color;
    const own=actor.relation==="self";
    canvas.beginPath();
    if(actor.relation!=="enemy"){
      canvas.arc(x,y,own ? 9 : 6.5,0,Math.PI*2);
    }else{
      canvas.moveTo(x,y-8);
      canvas.lineTo(x+7.5,y);
      canvas.lineTo(x,y+8);
      canvas.lineTo(x-7.5,y);
      canvas.closePath();
    }
    canvas.fillStyle=color;
    canvas.strokeStyle=own ? "#eafff8" : "#122124";
    canvas.lineWidth=own ? 2.8 : 2.1;
    canvas.fill();
    canvas.stroke();
  }

  function refreshReplay(){
    const tracks=selectedReplayTracks();
    const visible=tracks.filter(({marks})=>currentPosition(markIndex(replay.time,marks),marks)).length;
    range.value=replay.time;
    const stamp=clock(replay.time);
    if(el("replay-current").textContent!==stamp){
      el("replay-current").textContent=stamp;
      range.setAttribute("aria-valuetext",stamp+" / "+clock(replay.match.duration));
    }
    const beforeDeath=replay.stopAt===null ? null : Math.max(0,Math.ceil(replay.stopAt-replay.time));
    const status=beforeDeath!==null ? (beforeDeath ? t("ก่อนตาย ")+beforeDeath+t(" วินาที") : t("จุดตาย"))
      : replay.time>=replay.match.duration ? t("จบเกม") : tracks.length===0 ? t("เลือกฮีโร่เพื่อดูเส้นทาง")
      : t("ฮีโร่บนแผนที่ ")+visible+" / "+tracks.length+t(" ตัว");
    const caption=replay.match.games.length>1 ? status+t(" · เทียบ ")+replay.match.games.length+t(" เกมจาก 0:00") : status;
    if(el("replay-status").textContent!==caption) el("replay-status").textContent=caption;
    updateCombatFocus();
    draw();
  }

  function stopReplay(){
    cancelAnimationFrame(replay.frame);
    replay.frame=0;
    replay.last=null;
    playButton.textContent=t("เล่น");
    playButton.setAttribute("aria-pressed","false");
  }

  function playFrame(now){
    const end=replay.stopAt ?? replay.match.duration;
    if(replay.last!==null){
      const elapsed=Math.min((now-replay.last)/1000,.25);
      replay.time=Math.min(replay.time+elapsed*replay.rate,end);
    }
    replay.last=now;
    refreshReplay();
    if(replay.time>=end){
      stopReplay();
      replay.stopAt=null;
    }else{
      replay.frame=requestAnimationFrame(playFrame);
    }
  }

  function startReplay(){
    playButton.textContent=t("หยุด");
    playButton.setAttribute("aria-pressed","true");
    replay.frame=requestAnimationFrame(playFrame);
  }

  function setReplayMatch(){
    stopReplay();
    replay.stopAt=null;
    const match=scopes.get(el("match").value);
    replay.match=match;
    replay.routes=new Map(match.roster.flatMap(group=>group.entries.map(({match,actor,routeKey})=>
      [routeKey,makeRoute(actor,match)])));
    replay.selected=selectionFor(match,true);
    renderRoster(match,replay.selected,selectVisibleHeroes);
    replay.time=0;
    range.min=match.start;
    range.max=match.duration;
    const markers=el("replay-death-times");
    markers.replaceChildren();
    for(const d of match.games.flatMap(game=>game.deaths)){
      const option=document.createElement("option");
      option.value=d.t;
      markers.append(option);
    }
    el("replay-duration").textContent=clock(match.duration);
    render();
    range.setAttribute("aria-valuetext",clock(replay.time)+" / "+clock(match.duration));
    refreshReplay();
  }

  function switchReplay(targetMatch){
    if(replay.match){
      stopReplay();
      replay.stopAt=null;
      replay.match=null;
      replay.routes.clear();
      replay.selected=new Set();
      el("match").value=replay.returnMatch;
      render();
    }else{
      replay.returnMatch=el("match").value;
      if(targetMatch) el("match").value=targetMatch;
      setReplayMatch();
    }
    const active=Boolean(replay.match);
    el("replay-controls").hidden=!active;
    el("death-legend-text").textContent=active ? t("จุดตายของเรา") : t("จุดตาย");
    el("replay-legend").hidden=!active;
    el("overview-heat-legend").hidden=active;
    el("overview-mid-legend").hidden=active;
    el("phase-control").hidden=active;
    el("layer-control").hidden=active;
    toggleButton.textContent=active ? t("กลับฮีตแมป") : t("รีเพลย์เส้นทาง");
    toggleButton.setAttribute("aria-expanded",String(active));
  }

  function navigateDeath(key,fromList=false){
    const selected=view.deaths.find(row=>row.key===key);
    if(!selected) return;
    const entering=!replay.match;
    if(entering) switchReplay(selected.m.id);
    else if(replay.match.id!==selected.m.id){
      el("match").value=selected.m.id;
      setReplayMatch();
    }
    stopReplay();
    replay.time=Math.max(selected.m.start,selected.d.t-15);
    replay.stopAt=selected.d.t;
    choose(key);
    refreshReplay();
    startReplay();
    if(fromList && entering) playButton.focus({preventScroll:true});
    if(fromList && matchMedia("(max-width:870px)").matches){
      el("replay-controls").scrollIntoView({block:"start"});
    }
  }

  toggleButton.addEventListener("click",()=>switchReplay());
  playButton.addEventListener("click",()=>{
    if(replay.frame){
      stopReplay();
      return;
    }
    if(replay.time>=replay.match.duration) replay.time=0;
    if(replay.stopAt===null) choose(null);
    startReplay();
  });
  range.addEventListener("input",()=>{
    stopReplay();
    replay.stopAt=null;
    replay.time=Number(range.value);
    if(view.key) choose(null);
    refreshReplay();
  });
  for(const button of speedButtons){
    button.addEventListener("click",()=>{
      replay.rate=Number(button.value);
      for(const field of speedButtons) field.setAttribute("aria-pressed",String(field===button));
    });
  }
  document.addEventListener("visibilitychange",()=>{
    if(document.hidden && replay.frame) stopReplay();
  },{signal});

  const picker=el("match-picker");
  const pickButton=el("pick-matches");
  const applyButton=el("apply-matches");
  let matchBoxes=[];
  function makeMatchBox(match){
    const label=document.createElement("label");
    label.className="match-choice";
    const input=document.createElement("input");
    input.type="checkbox";
    input.name="match";
    input.value=match.id;
    const caption=document.createElement("span");
    caption.textContent=match.id+" · "+match.hero+" · "+patchLabel(match.patch)+" · "+clock(match.duration);
    label.append(input,caption);
    el("match-options").append(label);
    return input;
  }
  function pickedMatches(){
    return new Set(matchBoxes.filter(input=>input.checked).map(input=>input.value));
  }
  function updateMatchPicker(){
    const count=pickedMatches().size;
    applyButton.disabled=count===0;
    applyButton.textContent=count ? t("ใช้ ")+count+t(" เกม") : t("ใช้เกมที่เลือก");
  }
  function closeMatchPicker(){
    picker.hidden=true;
    pickButton.setAttribute("aria-expanded","false");
    pickButton.textContent=t("เลือกหลายเกม");
  }
  function changeMatch(){
    closeMatchPicker();
    if(replay.match) setReplayMatch();
    else render();
  }
  pickButton.addEventListener("click",()=>{
    if(!picker.hidden){
      closeMatchPicker();
      return;
    }
    const value=el("match").value;
    const custom=scopes.get("custom");
    const games=value==="all" ? (custom?.games ?? []) : scopes.get(value).games;
    const ids=new Set(games.map(game=>game.id));
    for(const input of matchBoxes) input.checked=ids.has(input.value);
    updateMatchPicker();
    picker.hidden=false;
    pickButton.setAttribute("aria-expanded","true");
    pickButton.textContent=t("ปิดตัวเลือกเกม");
  });
  picker.addEventListener("change",updateMatchPicker);
  picker.addEventListener("keydown",event=>{
    if(event.key==="Escape"){
      closeMatchPicker();
      pickButton.focus();
    }
  });
  picker.addEventListener("submit",event=>{
    event.preventDefault();
    const selected=pickedMatches();
    if(selected.size===0) return;
    const games=matches.filter(game=>selected.has(game.id));
    let value=games[0].id;
    if(games.length===matches.length){
      value="all";
    }else if(games.length>1){
      const scope=makeScope(games);
      scopes.set("custom",scope);
      const option=el("custom-match-option");
      option.hidden=false;
      option.textContent=t("เลือกเอง · ")+games.length+t(" เกม");
      value="custom";
    }
    el("match").value=value;
    changeMatch();
    pickButton.focus({preventScroll:true});
  });
  el("match").addEventListener("change",changeMatch);
  el("phase").addEventListener("change",render);
  el("layer").addEventListener("change",draw);
  const patchSelect=el("patch");
  const layouts=[...report.maps].sort((a,b)=>patchLabel(b.patch).localeCompare(patchLabel(a.patch),"en",{numeric:true}));
  layouts.forEach((entry,index)=>patchSelect.add(new Option(patchLabel(entry.patch),String(index))));
  function selectPatch(){
    const next=layouts[Number(patchSelect.value)];
    const wasReplaying=Boolean(replay.match);
    if(wasReplaying) stopReplay();
    layout=next;
    calibration=next.calibration;
    matches=report.matches.filter(match=>match.patch===next.patch);
    map.width=mapBase.width=next.width;
    map.height=mapBase.height=next.height;
    map.parentElement.style.aspectRatio=next.width+" / "+next.height;
    el("map-patch").textContent=t("แพตช์เกม")+" "+patchLabel(next.patch);
    heatLayer=null;
    loadBasemap();
    if(next.map){
      const background=new Image();
      background.addEventListener("load",()=>{
        if(layout!==next || signal.aborted) return;
        loadBasemap(background);
        draw();
      },{ once:true, signal });
      background.addEventListener("error",()=>{
        if(layout===next) el("map-patch").append(" · "+t("โหลดภาพแผนที่ไม่สำเร็จ"));
      },{ once:true, signal });
      background.src=next.map;
    }
    const matchSelect=el("match");
    const fixedOptions=[el("all-matches"),el("custom-match-option")];
    fixedOptions[1].hidden=true;
    matchSelect.replaceChildren(...fixedOptions);
    for(const match of matches){
      matchSelect.add(new Option(match.hero+" · "+match.team+" · "+patchLabel(match.patch)+" · "+clock(match.duration)+" · "+match.id,match.id));
    }
    matchSelect.value="all";
    scopes.clear();
    scopes.set("all",makeScope(matches));
    for(const match of matches) scopes.set(match.id,makeScope([match]));
    el("match-options").replaceChildren();
    matchBoxes=matches.map(makeMatchBox);
    closeMatchPicker();
    matchSelect.disabled=!matches.length;
    pickButton.disabled=!matches.length;
    toggleButton.disabled=!matches.length;
    if(wasReplaying){
      replay.returnMatch="all";
      if(matches.length) setReplayMatch();
      else switchReplay();
    }else render();
  }
  patchSelect.addEventListener("change",selectPatch);
  map.addEventListener("click",event=>{
    if(!replay.match && el("layer").value==="heat") return;
    const box=map.getBoundingClientRect();
    const u=(event.clientX-box.left)*map.width/box.width;
    const v=(event.clientY-box.top)*map.height/box.height;
    const hits=[];
    for(const row of view.deaths){
      if(replay.match&&row.d.t>replay.time) continue;
      const [x,y]=projected(row.d.x,row.d.y);
      const distance=Math.hypot(x-u,y-v);
      if(distance<=14) hits.push({row,distance});
    }
    if(!hits.length) return;
    hits.sort((a,b)=>a.distance-b.distance);
    const previous=hits.findIndex(hit=>hit.row.key===view.key);
    const selected=hits[(previous+1)%hits.length].row;
    navigateDeath(selected.key);
    const list=el("events");
    const button=Array.from(list.querySelectorAll("button")).find(item=>item.dataset.key===selected.key);
    if(button){
      const offset=button.getBoundingClientRect().top-list.getBoundingClientRect().top;
      if(offset<0||offset+button.offsetHeight>list.clientHeight) list.scrollTop+=offset-24;
    }
  });
  const latestWithMatches=layouts.findIndex(entry=>report.matches.some(match=>match.patch===entry.patch));
  patchSelect.value=String(latestWithMatches);
  selectPatch();
  return () => {
    controller.abort();
    stopReplay();
    root.replaceChildren();
  };
}
