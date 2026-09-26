import pytest

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source


@pytest.mark.parametrize('name', ['QF_CB15_ScorchedEarth_003E271D', 'QF_E06_Colossus_00583D14'])
def test_nuclear_fragments_preserve_generated_bindings_and_forward_to_tales(name):
    script = f'Fragments:Quests:{name}'
    patch = _script_patch_source(script)
    assert patch and 'Scriptname' not in patch
    skeleton = f'Scriptname {script} extends Quest\nReferenceAlias Property ExistingAlias Auto\n'
    merged = _merge_script_method_patches(skeleton, patch)
    assert 'ReferenceAlias Property ExistingAlias Auto' in merged
    assert merged.count(f'Scriptname {script}') == 1
    assert 'CastAs("B21:B21_TFA_NukeBossQuest")' in merged
    assert 'CallFunction("HandleStage", args)' in merged
    assert _merge_script_method_patches(merged, patch) == merged
