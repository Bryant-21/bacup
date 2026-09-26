import struct

import pytest

from bacup.scripts.export_nuke_zone_catalog import add_master_leaves, allowed, build_catalog, entries, leaves


def condition(function, parameter=0, flags=0):
    value = bytearray(32)
    value[0] = flags
    struct.pack_into('<fHI', value, 4, 1.0, function, 0)
    struct.pack_into('<H', value, 8, function)
    struct.pack_into('<I', value, 12, parameter)
    return bytes(value)


def record(eid, rows):
    fields = []
    for form, conditions, count in rows:
        fields.extend([{'LVLO': {'value': form}}, {'Quantity': count}])
        fields.extend({'CTDA': {'raw_hex': c.hex()}} for c in conditions)
    return {'eid': eid, 'fields': fields}


def test_nuke_conditions_respect_and_or_and_zero_quantity():
    nuke = condition(849)
    event = condition(74, 100)
    row = {'count': 1, 'conditions': [nuke]}
    assert allowed(row, True) and not allowed(row, False)
    assert not allowed({'count': 1, 'conditions': [nuke, event]}, True)
    assert allowed({'count': 1, 'conditions': [condition(849, flags=1), event]}, True)
    assert not allowed({'count': 0, 'conditions': [nuke]}, True)
    assert not allowed({'count': 1, 'conditions': [condition(875, 0x8464EE)]}, True)
    assert not allowed({'count': 1, 'conditions': [condition(579, 0x2D3EBF)]}, True)
    assert allowed({'count': 1, 'conditions': [condition(875, 0x8464EF)]}, True)


@pytest.mark.parametrize('key,wrapped', [('BaseDataNPC', False), ('BaseDataItem', True)])
def test_legacy_leveled_entry_shapes(key, wrapped):
    value = {'BaseDataLevel': 30, key: {'reference': {'plugin': 'SeventySix.esm', 'object_id': '001ABC'}}, 'BaseDataCount': 2}
    if wrapped:
        value = {'Unnamed': {'variant': 'base_data', 'value': value}}
    assert entries({'fields': [{'LVLO': {'value': value}}]}) == [
        {'form': 0x1ABC, 'level': 30, 'count': 2, 'conditions': []}]


def test_nested_lists_cycles_and_disabled_nuke_rows():
    lists = {1: record('LCharExample', [(2, [condition(849)], 1), (3, [], 1)]),
             2: record('Glowing', [(4, [], 1), (2, [], 1), (5, [condition(849)], 0)]),
             3: record('Normal', [(6, [], 1)])}
    assert leaves(1, lists, True) == [(4, 1)]
    assert leaves(1, lists, False) == [(6, 1)]


def test_already_nuked_and_placeholder_bases_are_not_replaced():
    lists = {1: record('LPI_FloraExample', [(2, [condition(849)], 1), (3, [], 1), (4, [], 1), (2, [], 1)])}
    converted = {2: {'eid': 'FloraRadExample'}, 3: {'eid': 'UseLPI_FloraExample'}, 4: {'eid': 'FloraDummy'}}
    catalog = build_catalog(lists, converted, {i: 'FLOR' for i in converted})
    assert [row['normal'] for row in catalog['mappings']] == [3]
    assert catalog['disabled_flora'] == [2]


def test_specific_family_beats_broader_variant_pool():
    lists = {1: record('LCharAll', [(9, [condition(849)], 1), (3, [], 1)]),
             2: record('LCharSpecific', [(4, [condition(849)], 1), (3, [], 1)]),
             9: record('GlowingAll', [(4, [], 1), (5, [], 1)])}
    converted = {i: {'eid': f'EncCreature{i}'} for i in (3, 4, 5)}
    catalog = build_catalog(lists, converted, {i: 'NPC_' for i in converted})
    assert [v['form'] for v in catalog['mappings'][0]['variants']] == [4]
    assert len(catalog['rejected']) == 1


def test_master_remapping_uses_editor_identity_and_real_form_key():
    source = {100: {'eid': 'EncMolerat03'}, 101: {'eid': 'EncGlowing'}}
    converted = {101: {'eid': 'EncGlowing'}}
    signatures = {101: 'NPC_'}
    donor = {'eid': 'EncMolerat03', 'signature': 'NPC_', 'form_key': 'Fallout4.esm:1832F7'}
    assert add_master_leaves(source, converted, signatures, [[donor], [donor]]) == []
    catalog = build_catalog({1: record('LCharMolerat', [(101, [condition(849)], 1), (100, [], 1)])}, converted, signatures)
    row = catalog['mappings'][0]
    assert (row['normal_plugin'], row['normal']) == ('Fallout4.esm', 0x1832F7)
    assert row['variants'] == [{'plugin': 'SeventySix.esm', 'form': 101, 'level': 1}]


def test_identity_mismatch_fails_and_ambiguous_donors_stay_unresolved():
    with pytest.raises(ValueError, match='identity mismatch'):
        add_master_leaves({1: {'eid': 'Original'}}, {1: {'eid': 'Wrong'}}, {}, [])
    donors = [{'eid': 'Original', 'signature': 'NPC_', 'form_key': f'{plugin}:000001'} for plugin in ('A.esm', 'B.esm')]
    assert add_master_leaves({1: {'eid': 'Original'}}, {}, {}, [donors]) == [1]
