#!/usr/bin/env python3
"""Bundle the reviewable design board with tokens, original vectors and offline fonts."""
from __future__ import annotations

import argparse
import base64
import hashlib
from html import escape
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BOARD = ROOT / "design/board"


def build() -> tuple[str, dict]:
    source = BOARD / "index.html"
    html = source.read_text(encoding="utf-8")
    inputs = [source, BOARD / "board.css", BOARD / "board.js", ROOT / "design/tokens/tokens.css", ROOT / "design/tokens/kohbor.tokens.json"]
    for name, path in (("tokens", inputs[3]), ("board", inputs[1])):
        css = path.read_text(encoding="utf-8")
        html = re.sub(r'<link rel="stylesheet"[^>]+data-embed="' + name + r'">', lambda _: '<style data-embed="' + name + '">\n' + css + '\n</style>', html)
    font_css = []
    for family, weight, name in (("Noto Sans Thai", 400, "NotoSansThai-Regular.ttf"), ("Noto Sans Thai", 700, "NotoSansThai-Bold.ttf"), ("Noto Sans", 400, "NotoSans-Regular.ttf"), ("Noto Sans", 700, "NotoSans-Bold.ttf")):
        path = ROOT / "design/assets/fonts" / name
        inputs.append(path)
        encoded = base64.b64encode(path.read_bytes()).decode("ascii")
        font_css.append(f'@font-face{{font-family:"{family}";font-style:normal;font-weight:{weight};font-display:swap;src:url(data:font/ttf;base64,{encoded}) format("truetype")}}')
    html = html.replace('</head>', '<style data-embed="fonts">' + '\n'.join(font_css) + '</style>\n</head>')
    font_license = ROOT / 'design/assets/fonts/OFL-Noto.txt'
    inputs.append(font_license)
    license_html = '<details class="font-license"><summary>Bundled font license</summary><pre>' + escape(font_license.read_text(encoding='utf-8')) + '</pre></details>'
    html = html.replace('</footer>', license_html + '</footer>')
    def embed_asset(match):
        path = (BOARD / match.group(1)).resolve()
        path.relative_to(ROOT / "design/assets")
        if path not in inputs:
            inputs.append(path)
        uri = 'data:image/svg+xml;base64,' + base64.b64encode(path.read_bytes()).decode('ascii')
        return 'src="' + uri + '" data-embed-asset'
    html = re.sub(r'src="([^"]+)" data-embed-asset', embed_asset, html)
    token_data = inputs[4].read_text(encoding="utf-8").replace('<', '\\u003c')
    html = re.sub(r'(<script id="kb-token-data" type="application/json">).*?(</script>)', lambda m: m[1] + token_data + m[2], html, flags=re.S)
    js = inputs[2].read_text(encoding="utf-8")
    if '</script' in js.lower():
        raise ValueError('Unsafe inline script closing tag')
    html = html.replace('<script src="board.js" data-embed="script"></script>', '<script data-embed="script">\n' + js + '\n</script>')
    if re.search(r'<(?:script[^>]+src|link[^>]+href)=', html):
        raise ValueError('External executable dependency in bundle')
    report = {
        'artifact': 'Kohbor design board', 'design_version': '1.0.0', 'requirements_baseline': '0.3.1',
        'evidence_class': 'static_bundle', 'status': 'PASS',
        'checks': ['styles embedded', 'script embedded', 'original SVG assets embedded', 'Thai and Latin regular/bold fonts embedded', 'font copyright and OFL license embedded', 'DTCG token download embedded'],
        'font_license': 'design/assets/fonts/OFL-Noto.txt',
        'files': [{'path': p.relative_to(ROOT).as_posix(), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in inputs],
        'output': {'path': 'design/board/kohbor-design-board.html', 'sha256': hashlib.sha256(html.encode()).hexdigest()},
        'application_native_live_acoustic_resource_tests': 'NOT_RUN',
    }
    return html, report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    html, report = build()
    outputs = {BOARD / 'kohbor-design-board.html': html, BOARD / 'bundle-validation.json': json.dumps(report, ensure_ascii=False, indent=2) + '\n'}
    if args.check:
        stale = [p.relative_to(ROOT).as_posix() for p, content in outputs.items() if not p.exists() or p.read_text(encoding='utf-8') != content]
        if stale:
            print(json.dumps({'status': 'FAIL', 'stale': stale})); return 1
    else:
        for path, content in outputs.items(): path.write_text(content, encoding='utf-8')
    print(json.dumps({'status': 'PASS', 'mode': 'check' if args.check else 'build', 'scope': 'offline design document bundle', 'output': list(str(p.relative_to(ROOT)) for p in outputs)}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
