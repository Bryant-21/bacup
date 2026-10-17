from imgui_bundle import imgui

from creation_lib.mod_manager.status_query import OPTIONS


def draw(workspace):
    imgui.text_unformatted("Status")
    for name, options in OPTIONS.items():
        keys, labels = zip(*options)
        imgui.set_next_item_width(-1)
        changed, index = imgui.combo("##status_" + name, keys.index(workspace._status_filters[name]), list(labels))
        if changed:
            workspace._status_filters[name] = keys[index]
        imgui.set_item_tooltip("Loose conflicts need Refresh files. Updates/account states need Nexus refresh in Information. Unknown stays distinct from a measured result; target source overrides can clear file identity.")
