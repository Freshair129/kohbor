#!/usr/bin/env python3
"""Synchronize the SRS requirement section, RTM and Gherkin from one baseline."""
import argparse
import csv
import io
import json
import os
from pathlib import Path

BEGIN = '<!-- BEGIN GENERATED REQUIREMENTS -->'
END = '<!-- END GENERATED REQUIREMENTS -->'
SRS = 'docs/requirements/SRS.md'
RTM = 'docs/requirements/RTM.csv'


def link(path, label=None):
    relative = os.path.relpath(path, Path(SRS).parent).replace(os.sep, '/')
    return '[' + (label or Path(path).name) + '](' + relative + ')'


def render(root):
    data = json.loads((root / 'contracts/requirements.json').read_text(encoding='utf-8'))
    rows = data['requirements']
    parts = [
        '\n\nThis controlled section and the structured registry represent the same requirements baseline. ',
        'Edit an atomic row through the registry, regenerate, and review the complete SRS context for consistency. ',
        'The registry is an authoring mechanism, not a higher authority. Acceptance scenarios are test designs; ',
        'new-application evidence is NOT_RUN until actual execution is recorded.\n\n'
    ]
    groups = list(dict.fromkeys(r['module'] for r in rows))
    for group in groups:
        parts.append('### ' + group.upper() + '\n\n')
        for row in rows:
            if row['module'] != group:
                continue
            acceptance = row['acceptance']
            verification = row['verification']
            parts.append(f"#### {row['id']} — {row['title']}\n\n")
            parts.append(f"**Scope:** {row['scope']} · **Priority:** {row['priority']} · **Basis:** {row['basis']} / {row['decision_basis']}\n\n")
            parts.append(f"**Product needs:** {', '.join(row['product_need_ids'])} · **Primary design:** {row['primary_design_component_id']} · **Design components:** {', '.join(row['design_component_ids'])}\n\n")
            parts.append(row['statement'] + '\n\n')
            parts.append(f"**Failure behavior:** {row['failure_behavior']}\n\n")
            parts.append(f"**{acceptance['scenario_id']}:** Given {acceptance['given']}; when {acceptance['when']}; then {acceptance['then']}.\n\n")
            parts.append(f"**Verification:** {', '.join(verification['required_evidence'])}; procedures {', '.join(verification['procedure_ids'])}; status {verification['new_build_status']}.\n\n")
            if row['open_decisions']:
                parts.append('**Open dependencies:** ' + ', '.join(row['open_decisions']) + '.\n\n')
            parts.append('**Elaboration:** ' + ', '.join(link(p) for p in row['spec_refs']) + '\n\n')
            if row['contract_refs']:
                parts.append('**Contracts:** ' + ', '.join(link(p) for p in row['contract_refs']) + '\n\n')
            if row['diagram_ids']:
                parts.append('**Diagram views:** ' + ', '.join(link(p, identifier) for identifier, p in zip(row['diagram_ids'], row['diagram_refs'])) + '\n\n')
            parts.append('**Implementation freedom:** ' + row['implementation_freedom'] + '\n\n')
    existing = (root / SRS).read_text(encoding='utf-8')
    if existing.count(BEGIN) != 1 or existing.count(END) != 1 or existing.index(BEGIN) > existing.index(END):
        raise ValueError('SRS must contain one ordered pair of generated-section markers.')
    prefix, remaining = existing.split(BEGIN, 1)
    _, suffix = remaining.split(END, 1)
    srs = prefix + BEGIN + ''.join(parts) + END + suffix

    output = io.StringIO(newline='')
    writer = csv.writer(output, lineterminator="\n")
    writer.writerow([
        'product_need_ids', 'requirement_id', 'module', 'scope', 'priority',
        'primary_design_component_id', 'design_component_ids', 'design_refs', 'contract_refs', 'spec_refs', 'diagram_ids', 'diagram_refs',
        'acceptance_scenario', 'procedure_ids', 'procedure_refs', 'required_evidence',
        'implementation_refs', 'evidence_refs', 'open_decisions', 'new_build_status'
    ])
    for row in rows:
        verification = row['verification']
        writer.writerow([
            ';'.join(row['product_need_ids']), row['id'], row['module'], row['scope'], row['priority'],
            row['primary_design_component_id'], ';'.join(row['design_component_ids']), ';'.join(row['design_refs']),
            ';'.join(row['contract_refs']), ';'.join(row['spec_refs']), ';'.join(row['diagram_ids']), ';'.join(row['diagram_refs']), row['acceptance']['scenario_id'],
            ';'.join(verification['procedure_ids']), ';'.join(verification['procedure_refs']),
            ';'.join(verification['required_evidence']), ';'.join(verification['implementation_refs']),
            ';'.join(verification['evidence_refs']), ';'.join(row['open_decisions']), verification['new_build_status']
        ])
    lines = [
        '# Test designs only: application step bindings and actual execution are still required.',
        '# These scenarios do not claim native, live, acoustic or resource PASS.',
        'Feature: Brand-neutral desktop companion target behavior', ''
    ]
    for row in rows:
        acceptance = row['acceptance']
        lines += [
            '  @' + row['id'] + ' @' + row['scope'],
            '  Scenario: ' + acceptance['scenario_id'] + ' ' + row['title'],
            '    Given ' + acceptance['given'], '    When ' + acceptance['when'],
            '    Then ' + acceptance['then'], '    And failure behavior is: ' + row['failure_behavior'], ''
        ]
    return {SRS: srs, RTM: output.getvalue(), 'acceptance/scenarios.feature': '\n'.join(lines) + '\n'}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    try:
        expected = render(root)
    except (KeyError, ValueError, OSError) as error:
        print('RENDER_ERROR: ' + str(error))
        return 1
    if args.check:
        stale = [name for name, content in expected.items()
                 if not (root / name).exists() or (root / name).read_bytes() != content.encode('utf-8')]
        if stale:
            print('STALE_GENERATED_VIEWS: ' + ', '.join(stale))
            return 1
        print('SRS detailed requirements, RTM and Gherkin are synchronized.')
        return 0
    for name, content in expected.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_bytes(content.encode('utf-8'))
    print('Synchronized three artifacts; handwritten SRS context preserved.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
