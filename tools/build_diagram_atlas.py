#!/usr/bin/env python3
"""Build the Markdown index and offline HTML atlas from reviewed catalog + SVGs."""
from pathlib import Path
import base64
import hashlib
import html
import json

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'docs/diagrams'
catalog = json.loads((DIRECTORY / 'catalog.json').read_text(encoding='utf-8'))
report = json.loads((ROOT / 'acceptance/diagram-validation.json').read_text(encoding='utf-8'))
results = {d['id']: d for d in report['diagrams']}
esc = html.escape
nav, articles, table = [], [], []
for diagram in catalog['diagrams']:
    result = results[diagram['id']]
    if result['status'] != 'PASS':
        raise ValueError('Unrendered diagram: ' + diagram['id'])
    source = (ROOT / diagram['source']).read_text(encoding='utf-8')
    svg = (ROOT / diagram['rendered']).read_bytes()
    if hashlib.sha256(source.encode()).hexdigest() != result['source_sha256'] or hashlib.sha256(svg).hexdigest() != result['svg_sha256']:
        raise ValueError('Stale diagram rendering: ' + diagram['id'])
    source_relative = Path(diagram['source']).relative_to('docs/diagrams').as_posix()
    svg_relative = Path(diagram['rendered']).relative_to('docs/diagrams').as_posix()
    owner_relative = '../' + diagram['owner_doc'].removeprefix('docs/')
    table.append(f"| {diagram['id']} | [{diagram['title']}]({svg_relative}) | {diagram['kind']} | [{Path(diagram['owner_doc']).name}]({owner_relative}) | [Mermaid]({source_relative}) |")
    search_text = ' '.join([diagram['id'], diagram['title'], diagram['title_th'], diagram['kind'], *diagram['requirement_ids'], *diagram['decision_ids']])
    nav.append(f'<button class="nav-item" type="button" data-target="{diagram["id"]}" data-search="{esc(search_text, quote=True)}"><small>{diagram["id"]} · {esc(diagram["kind"])}</small><span>{esc(diagram["title_th"])}</span></button>')
    data_url = 'data:image/svg+xml;base64,' + base64.b64encode(svg).decode()
    notes = ''.join('<li>' + esc(note) + '</li>' for note in diagram['notes'])
    requirements = ', '.join(diagram['requirement_ids'])
    decisions = ', '.join(diagram['decision_ids']) or '—'
    articles.append(f'''<article class="diagram" id="{diagram['id']}" hidden>
      <header><div class="eyebrow">{diagram['id']} · {esc(diagram['kind'])}</div><h2>{esc(diagram['title_th'])}</h2><p class="english-title">{esc(diagram['title'])}</p><p>{esc(diagram['purpose'])}</p></header>
      <div class="figure-controls"><button type="button" data-fit>พอดีความกว้าง</button><button type="button" data-actual>ขนาดจริง</button><a download="{esc(Path(diagram['rendered']).name, quote=True)}" href="{data_url}">ดาวน์โหลด SVG</a></div>
      <div class="canvas" tabindex="0" aria-label="Diagram viewport"><img src="{data_url}" data-native-width="{result['width']}" alt="{esc(diagram['title'], quote=True)}"></div>
      <section class="reading-notes"><h3>เงื่อนไขที่ต้องอ่านร่วมกับภาพ</h3><ul>{notes}</ul><dl><dt>เอกสารหลักใน ZIP</dt><dd><code>{esc(diagram['owner_doc'])}</code></dd><dt>SRS requirements</dt><dd>{esc(requirements)}</dd><dt>การตัดสินใจที่เกี่ยวข้อง</dt><dd>{esc(decisions)}</dd></dl></section>
      <details><summary>Mermaid ต้นฉบับที่แก้ไขได้</summary><pre><code>{esc(source)}</code></pre></details>
    </article>''')

css = '''
*{box-sizing:border-box}html{scroll-behavior:smooth}body{margin:0;background:#f6f8fb;color:#1d2d43;font:15px/1.65 system-ui,-apple-system,"Segoe UI",sans-serif}button,input{font:inherit}a{color:#2158a0}button{cursor:pointer}button:focus-visible,a:focus-visible,input:focus-visible,.canvas:focus-visible{outline:3px solid #3f8ade;outline-offset:2px}[hidden]{display:none!important}
.top{background:#fff;border-bottom:1px solid #dce3ec;padding:26px 34px 20px}.top h1{font-size:27px;letter-spacing:-.03em;margin:0 0 4px}.top p{margin:0;color:#53677f}.top .status{font-size:12px;letter-spacing:.07em;text-transform:uppercase;color:#6e5b2d;margin-bottom:6px}.layout{display:grid;grid-template-columns:292px minmax(0,1fr);max-width:1700px;margin:auto}aside{padding:22px 14px 30px 22px;position:sticky;top:0;align-self:start;height:100vh;overflow:auto;border-right:1px solid #dce3ec}label{font-size:13px;font-weight:600}#search{display:block;width:100%;margin:6px 0 8px;padding:10px;border:1px solid #aab8ca;border-radius:8px;background:white}#count{display:block;color:#62758b;font-size:12px;margin-bottom:13px}.nav-item{display:block;width:100%;text-align:left;border:1px solid transparent;background:transparent;border-radius:8px;padding:9px 10px;margin:3px 0;color:#2a405c}.nav-item small{display:block;font-size:10px;color:#6b7d93;letter-spacing:.03em}.nav-item span{display:block;font-size:13px}.nav-item:hover{background:#eaf0f8}.nav-item[aria-current=true]{background:#e4edfb;border-color:#bdcfea;color:#153f76}main{padding:26px 34px;min-width:0}.pager{display:flex;gap:8px;justify-content:space-between;margin-bottom:15px}.pager button,.figure-controls button,.figure-controls a{border:1px solid #b9c7d8;background:#fff;border-radius:7px;padding:6px 12px;text-decoration:none;font-size:12px;color:#294866}.pager button:disabled{opacity:.4;cursor:default}.eyebrow{font-size:11px;text-transform:uppercase;letter-spacing:.09em;color:#547398}h2{font-size:24px;margin:4px 0 1px;line-height:1.4;letter-spacing:-.02em}.english-title{color:#6a7d92;margin:0;font-size:14px}header p{max-width:940px}.figure-controls{display:flex;flex-wrap:wrap;gap:7px;padding:10px 0}.canvas{background:#fff;border:1px solid #d8e1ec;border-radius:10px;overflow:auto;max-height:76vh;padding:18px}.canvas img{display:block;width:100%;height:auto;max-width:none}.reading-notes{max-width:1060px;font-size:13px}.reading-notes h3{font-size:15px;margin:23px 0 7px}.reading-notes ul{padding-left:20px}.reading-notes li{margin:6px 0}.reading-notes dl{display:grid;grid-template-columns:180px minmax(0,1fr);border-top:1px solid #dce3ec;margin-top:18px;padding-top:12px;gap:8px}.reading-notes dt{color:#64758a}.reading-notes dd{margin:0;overflow-wrap:anywhere}details{border-top:1px solid #dce3ec;padding-top:12px;margin-top:20px;font-size:13px}summary{cursor:pointer;font-weight:600}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#edf1f7;border-radius:8px;padding:16px;font:12px/1.65 ui-monospace,Consolas,monospace}footer{font-size:12px;color:#65758a;border-top:1px solid #dce3ec;margin-top:30px;padding-top:16px}#empty{padding:30px 0;color:#667a93}
@media(max-width:800px){.top{padding:20px}.layout{grid-template-columns:1fr}aside{position:static;height:auto;max-height:310px;border-right:0;border-bottom:1px solid #dce3ec;padding:16px 20px}main{padding:22px 16px}.reading-notes dl{grid-template-columns:1fr;gap:2px}.reading-notes dd{margin-bottom:9px}h2{font-size:21px}.canvas{padding:8px}}
@media print{body{background:white}.top{padding:0 0 16px}aside,.pager,.figure-controls,details{display:none!important}.layout{display:block}main{padding:0}.diagram[hidden],.diagram{display:block!important;break-before:page}.canvas{max-height:none;overflow:visible;border:0;padding:0}.canvas img{width:100%!important;max-height:70vh;object-fit:contain}.reading-notes{font-size:10px}.reading-notes h3{font-size:12px}footer{display:none}}
'''
script = '''
const buttons=[...document.querySelectorAll('.nav-item')];const articles=[...document.querySelectorAll('.diagram')];let active='';
const visible=()=>buttons.filter(b=>!b.hidden);
function show(id){const button=buttons.find(b=>b.dataset.target===id&&!b.hidden);if(!button)return;active=id;articles.forEach(a=>a.hidden=a.id!==id);buttons.forEach(b=>b.setAttribute('aria-current',String(b===button)));history.replaceState(null,'','#'+id);const list=visible();const index=list.indexOf(button);document.getElementById('previous').disabled=index<=0;document.getElementById('next').disabled=index>=list.length-1;document.getElementById('position').textContent=(index+1)+' / '+list.length;}
buttons.forEach(b=>b.addEventListener('click',()=>show(b.dataset.target)));
document.getElementById('search').addEventListener('input',event=>{const q=event.target.value.trim().toLowerCase();buttons.forEach(b=>b.hidden=!b.dataset.search.toLowerCase().includes(q));const list=visible();document.getElementById('count').textContent=list.length+' แผนภาพ';document.getElementById('empty').hidden=list.length!==0;if(!list.length){articles.forEach(a=>a.hidden=true);document.getElementById('position').textContent='0 / 0';document.getElementById('previous').disabled=true;document.getElementById('next').disabled=true;}else show(list.some(b=>b.dataset.target===active)?active:list[0].dataset.target);});
for(const [id,offset] of [['previous',-1],['next',1]])document.getElementById(id).addEventListener('click',()=>{const list=visible();const i=list.findIndex(b=>b.dataset.target===active);if(list[i+offset])show(list[i+offset].dataset.target);});
document.querySelectorAll('[data-fit]').forEach(b=>b.addEventListener('click',()=>b.closest('article').querySelector('img').style.width='100%'));
document.querySelectorAll('[data-actual]').forEach(b=>b.addEventListener('click',()=>{const image=b.closest('article').querySelector('img');image.style.width=image.dataset.nativeWidth+'px';}));
document.getElementById('count').textContent=buttons.length+' แผนภาพ';show(buttons.some(b=>b.dataset.target===location.hash.slice(1))?location.hash.slice(1):buttons[0].dataset.target);
'''
html_text = f'''<!doctype html><html lang="th"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Engineering Diagram Atlas</title><style>{css}</style></head><body>
<div class="top"><div class="status">Specification {esc(catalog['spec_version'])} · Draft · Application tests NOT_RUN</div><h1>แผนภาพระบบ</h1><p>ภาพโครงสร้าง ข้อมูล และพฤติกรรม พร้อมต้นฉบับ Mermaid และข้อกำหนดอ้างอิง</p></div>
<div class="layout"><aside aria-label="Diagram navigation"><label for="search">ค้นหาภาพหรือ Requirement ID</label><input id="search" type="search" placeholder="เช่น voice, PEER-005, ข้อมูล"><span id="count"></span>{''.join(nav)}</aside>
<main><div class="pager"><button id="previous" type="button">← ก่อนหน้า</button><span id="position" aria-live="polite"></span><button id="next" type="button">ถัดไป →</button></div><p id="empty" hidden>ไม่พบแผนภาพที่ตรงกับคำค้น</p>{''.join(articles)}<footer>ภาพเหล่านี้เป็นมุมมองของสเปกฉบับร่าง อ่านเงื่อนไขใน SRS และเอกสารออกแบบร่วมด้วย ช่องว่างของ protocol/model/design ยังอยู่ใน Decision Log การ render ผ่านไม่ใช่ผลทดสอบแอปจริง หน้านี้เปิดดูได้โดยไม่ใช้เครือข่าย</footer></main></div><script>{script}</script></body></html>'''
(DIRECTORY / 'INDEX.html').write_text(html_text, encoding='utf-8')

intro = '''# Diagram Index and Maintenance

**Version 0.3.1 · Draft · Application evidence NOT_RUN**

Open [INDEX.html](INDEX.html) for the offline searchable atlas, or open the SVG links below. Each editable Mermaid source has one generated SVG. The catalog identifies its purpose, governing document, requirement IDs and open decisions. Read the notes in the atlas/catalog with the image: short labels cannot express every guard or invariant.

These are tailored engineering views: system context, containers, deployment, data flow, logical ERD, sequences, states and user/release flows. They are not a claim to implement every UML diagram type or every C4 level. A missing detailed protocol is still an open decision even when its interaction is pictured.

## Diagram inventory

| ID | Diagram | View | Governing document | Editable source |
| --- | --- | --- | --- | --- |
'''
tail = '''

## Authority and traceability

PRD/SRS and approved decisions govern the same baseline represented by these figures. A diagram cannot broaden data access, consent, scope or acceptance. A conflict with text is a defect to reconcile, not a choice between convenient interpretations. The [catalog](catalog.json) and [RTM](../requirements/RTM.csv) connect diagrams with requirements. Not every requirement needs a diagram; diagram coverage is not implementation or verification coverage.

The ERD is a logical model, not a frozen physical database schema. C4-style context/container views use generic Mermaid flowchart notation; a container denotes an executable application or data store, not necessarily Docker. Component responsibilities remain in SDD; context and deployment figures must not silently allocate new services.

## Update workflow

1. Resolve the relevant requirement/decision and edit the `.mmd` source plus catalog notes/mappings.
2. Regenerate SVGs with the pinned renderer; inspect readability and every failure/cancellation path affected.
3. Rebuild the atlas, regenerate SRS/RTM views if mappings change, and validate the package.
4. Commit source, catalog, rendered figures and updated traceability together. Do not hand-edit the SVG while leaving its source unchanged.

Renderer setup and commands, from the package root:

```bash
npm ci --prefix tools
node tools/render_diagrams.mjs
python tools/build_diagram_atlas.py
python tools/render_requirements.py
python tools/validate_spec.py
```

The rendering dependencies are documentation tools, not application runtime dependencies. The pinned Mermaid, jsdom and svgdom renderer runs locally under Node; it uses the unmodified DejaVu fonts bundled in `tools/fonts` with their license. Initial dependency installation requires npm access, but subsequent rendering requires no browser or render service. `--module-root` accepts an existing directory containing the pinned packages. Generated SVG and HTML require no JavaScript diagram engine or network to display the figures. The atlas's local navigation/search uses a small embedded script.

Text metrics come from the bundled fonts through svgdom rather than a browser layout engine. Keep the tested classic appearance and HTML labels disabled. After regeneration, rasterize the affected figures with an independent SVG viewer and inspect label spacing, line wrapping, box bounds and arrows; finite coordinates alone are not visual acceptance.

The render report checks syntax/output and hashes; it does not establish semantic correctness. Review the requirements and notes whenever a diagram changes. Native, live, acoustic and resource evidence remains NOT_RUN.

## Notation references

- [C4 diagram views](https://c4model.com/diagrams): choose the abstraction levels and supporting views that add value; this package does not claim formal C4 conformance.
- [Mermaid introduction](https://mermaid.js.org/intro/): text sources for editable diagrams. The package pins its renderer version in `tools/package.json` and keeps a generated SVG with every source.
'''
(DIRECTORY / 'README.md').write_text(intro + '\n'.join(table) + tail, encoding='utf-8')
print(json.dumps({'atlas': 'docs/diagrams/INDEX.html', 'diagrams': len(catalog['diagrams']), 'html_bytes': len(html_text.encode())}))
