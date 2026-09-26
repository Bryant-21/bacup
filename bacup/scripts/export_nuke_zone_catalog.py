from __future__ import annotations

import argparse
import json
import struct
import subprocess
from collections import defaultdict
from pathlib import Path


def entries(record):
    result = []
    for field in record['fields']:
        if 'LVLO' in field:
            value = field['LVLO']
            payload = value['value']
            if isinstance(payload, dict) and 'Unnamed' in payload:
                payload = payload['Unnamed']['value']
            if isinstance(payload, dict):
                reference = payload.get('BaseDataNPC', payload.get('BaseDataItem'))
                form = int(reference['reference']['object_id'], 16)
                level = payload.get('BaseDataLevel', 1)
                count = payload.get('BaseDataCount', 1)
            else:
                form, level, count = int(payload) & 0xFFFFFF, 1, 1
            result.append({'form': form, 'level': level, 'count': count, 'conditions': []})
        elif result:
            if 'CTDA' in field:
                result[-1]['conditions'].append(bytes.fromhex(field['CTDA']['raw_hex']))
            if 'MinimumLevel' in field:
                result[-1]['level'] = int(field['MinimumLevel'])
            if 'Quantity' in field:
                result[-1]['count'] = field['Quantity']
    return result


def is_nuke_condition(condition):
    if len(condition) < 16 or condition[0] & 4:
        return False
    function = struct.unpack_from('<H', condition, 8)[0]
    parameter = struct.unpack_from('<I', condition, 12)[0] & 0xFFFFFF
    comparison = struct.unpack_from('<f', condition, 4)[0]
    return condition[0] >> 5 == 0 and comparison == 1 and (
        function == 849 or function == 875 and parameter == 0x8464EF)


def allowed(entry, nuked):
    if entry['count'] <= 0:
        return False
    conditions = entry['conditions']
    if not conditions:
        return True
    group = []
    for condition in conditions:
        function = struct.unpack_from('<H', condition, 8)[0]
        group.append(function == 77 or nuked and is_nuke_condition(condition))
        if not condition[0] & 1:
            if not any(group):
                return False
            group = []
    return not group or any(group)


def leaves(form, lists, nuked, path=(), level=1):
    if form in path:
        return []
    if form not in lists:
        return [(form, level)]
    rows = entries(lists[form])
    explicit = [entry for entry in rows if allowed(entry, True) and any(is_nuke_condition(c) for c in entry['conditions'])]
    selected = explicit if nuked and explicit else [entry for entry in rows if allowed(entry, nuked)]
    return [leaf for entry in selected
            for leaf in leaves(entry['form'], lists, nuked, (*path, form), max(level, entry['level']))]


def candidates(lists):
    pairs = []
    for form, record in lists.items():
        if record['eid'].lower().startswith(('cut_', 'zzz', 'debug', 'donotuse')):
            continue
        if not any(is_nuke_condition(c) for e in entries(record) for c in e['conditions']):
            continue
        normal = set(leaves(form, lists, False))
        irradiated = set(leaves(form, lists, True))
        for original, _ in normal:
            if any(replacement == original for replacement, _ in irradiated):
                continue
            choices = sorted((replacement, level) for replacement, level in irradiated if replacement != original)
            if choices:
                pairs.append((form, original, choices))
    return pairs


def field(record, key, default=None):
    return next((f[key] for f in record['fields'] if key in f), default)


def add_master_leaves(source, converted, signatures, masters):
    names = defaultdict(dict)
    for rows in masters:
        for row in rows:
            names[row['eid'].lower()][row['form_key']] = row
    missing = []
    for form, original in source.items():
        if form in converted:
            if converted[form]['eid'].lower() not in (original['eid'].lower(), original['eid'].lower() + 'fo76'):
                raise ValueError(f"Converted identity mismatch: {original['eid']}")
            continue
        matches = list(names[original['eid'].lower()].values())
        if len(matches) != 1:
            missing.append(form)
            continue
        row = matches[0]
        plugin, local = row['form_key'].split(':')
        converted[form] = {**row, '_plugin': plugin, '_local': int(local, 16)}
        signatures[form] = row['signature']
    return missing


def build_catalog(lists, converted, signatures):
    mappings = {}
    rejected = []
    for source_list, normal, choices in sorted(candidates(lists), key=lambda row: (len(row[2]), row[0], row[1])):
        kind = 'actor' if signatures.get(normal) == 'NPC_' else 'flora'
        accepted_types = {'NPC_'} if kind == 'actor' else {'FLOR', 'ACTI'}
        if normal not in converted or signatures.get(normal) not in accepted_types:
            continue
        record = converted[normal]
        if any(token in record.get('eid', '').lower() for token in ('stubmarker', 'dummy', 'donotuse', 'debug')):
            continue
        if kind == 'flora' and signatures.get(normal) == 'ACTI' and 'flora' not in record['eid'].lower():
            continue
        valid = [(form, level) for form, level in choices
                 if form in converted and signatures.get(form) in accepted_types]
        if not valid:
            rejected.append({'list': source_list, 'normal': normal, 'reason': 'no converted nuke leaf'})
            continue
        value = {'normal': record.get('_local', normal), 'normal_plugin': record.get('_plugin', 'SeventySix.esm'),
                 'kind': kind, 'variants': [{'form': converted[form].get('_local', form),
                     'plugin': converted[form].get('_plugin', 'SeventySix.esm'), 'level': level} for form, level in valid]}
        if normal in mappings and mappings[normal]['variants'] != value['variants']:
            previous = {(v['plugin'], v['form'], v['level']) for v in mappings[normal]['variants']}
            current = {(v['plugin'], v['form'], v['level']) for v in value['variants']}
            if previous != current:
                rejected.append({'list': source_list, 'normal': normal, 'reason': 'alternate pool; narrower or earlier source list selected'})
                continue
        mappings[normal] = value
    enabled = []
    for form, record in converted.items():
        if signatures.get(form) in {'FLOR', 'ACTI'} and 'florarad' in record.get('eid', '').lower():
            enabled.append(form)
    return {'version': 1, 'plugin': 'SeventySix.esm', 'mappings': sorted(mappings.values(), key=lambda row: (row['normal_plugin'], row['normal'])),
            'disabled_flora': sorted(enabled), 'rejected': rejected}


def cli(output, *args):
    if '--game' not in args:
        args = ('--game', 'fo4', *args)
    result = subprocess.run(['modkit.exe', '--output', str(output), *args], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    report = json.loads(output.read_text(encoding='utf-8-sig'))
    if report.get('meta', {}).get('truncated'):
        raise ValueError(f'Incomplete CLI report: {output}')
    return report


def export(source, converted, output, cache, masters):
    cache.mkdir(parents=True, exist_ok=True)
    flora = cli(cache / 'flora-list-index.json', '--game', 'fo76', 'esp', 'query', source, '--type', 'LVLI', '--match', '*Flora*')['data']
    actors = cli(cache / 'actor-list-index.json', '--game', 'fo76', 'esp', 'query', source, '--type', 'LVLN')['data']
    all_items = cli(cache / 'source-all-list-index.json', '--game', 'fo76', '--fields', 'form_id,eid',
                    'esp', 'query', source, '--type', 'LVLI')['data']
    list_ids = {int(r['form_id'], 16) for r in [*all_items, *actors]}
    ids = sorted({r['form_id'] for r in [*flora, *actors]})
    ordered = cli(cache / 'source-lists.json', '--game', 'fo76', 'esp', 'get-records', source, *ids, '--authoring')
    assert not ordered['missing']
    lists = {int(r['form_id'], 16): r for r in ordered['records']}
    while missing := {e['form'] for r in lists.values() for e in entries(r)
                      if e['form'] in list_ids and e['form'] not in lists}:
        nested = cli(cache / 'nested-lists.json', '--game', 'fo76', 'esp', 'get-records', source,
                     *[f'{form:06X}' for form in sorted(missing)], '--authoring')
        assert not nested['missing']
        lists.update({int(r['form_id'], 16): r for r in nested['records']})
    (cache / 'resolved-lists.json').write_text(json.dumps(list(lists.values())), encoding='utf-8')
    pairs = candidates(lists)
    leaf_ids = {normal for _, normal, _ in pairs} | {form for _, _, choices in pairs for form, _ in choices}
    flora_bases = cli(cache / 'converted-rad-flora.json', 'esp', 'query', converted, '--type', 'FLOR', '--match', '*FloraRad*')['data']
    leaf_ids.update(int(r['form_id'], 16) for r in flora_bases)
    records = cli(cache / 'converted-leaves.json', 'esp', 'get-records', converted,
                  *[f'{form:06X}' for form in sorted(leaf_ids)], '--authoring')
    lookup = {int(r['form_id'].split(':')[0], 16): r for r in records['records']}
    signatures = {}
    for kind in ('NPC_', 'FLOR', 'ACTI'):
        index = cli(cache / f'converted-{kind}.json', '--fields', 'form_id,eid,signature', 'esp', 'query', converted, '--type', kind)
        signatures.update({int(r['form_id'].split(':')[0], 16): kind for r in index['data']})
    source_leaves = cli(cache / 'source-leaves.json', '--game', 'fo76', 'esp', 'get-records', source,
                       *[f'{form:06X}' for form in sorted(leaf_ids)], '--authoring')
    master_rows = [cli(cache / (master.name + '.json'), '--fields', 'form_id,eid,signature,form_key',
                      'esp', 'query', str(master), '--type', 'NPC_')['data'] for master in masters]
    unresolved = add_master_leaves({int(r['form_id'], 16): r for r in source_leaves['records']},
                                  lookup, signatures, master_rows)
    result = build_catalog(lists, lookup, signatures)
    result['source_lists'] = len(lists)
    result['missing_converted_forms'] = [f'{form:06X}' for form in unresolved]
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'output': str(output), 'mappings': len(result['mappings']),
                      'disabled_flora': len(result['disabled_flora']), 'rejected': len(result['rejected'])}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--converted', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--master', type=Path, action='append', default=[])
    args = parser.parse_args()
    export(str(args.source), str(args.converted), args.output, args.cache, args.master)
