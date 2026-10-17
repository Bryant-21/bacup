from types import SimpleNamespace
import threading

import pytest

from creation_lib.mod_manager import organization

from creation_lib.mod_manager import mod_drag
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


@pytest.fixture
def drag(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    profile = library.profiles()[0].id
    m = SimpleNamespace(library=library, profile_id=profile, busy=False, runtime_active=False)
    m.rows = library.rows(profile)
    m.rows_by_id = {row.id: row for row in m.rows}
    m.bulk_organize = lambda ids, **changes: organization.bulk(library, m.profile_id, ids, **changes)
    external = [row.id for row in m.rows if row.kind == "external"]
    chosen = {external[0], external[-1]}
    return SimpleNamespace(manager=m, _selected_mods=chosen), source


@pytest.mark.parametrize("after", [False, True])
def test_selected_block_moves_before_after_and_undo_without_payload_writes(drag, after):
    w, source = drag
    m = w.manager
    original = {str(p): p.read_bytes() for p in source.rglob("*") if p.is_file()}
    before = m.rows
    gesture = mod_drag.begin(w, next(iter(w._selected_mods)))
    target = next(row.id for row in reversed(before) if row.id not in gesture.mod_ids)
    mod_drag.drop(w, gesture, target=target, after=after)
    remaining = [row.id for row in before if row.id not in gesture.mod_ids]
    index = remaining.index(target) + int(after)
    assert [row.id for row in m.library.rows(m.profile_id)] == remaining[:index] + list(gesture.mod_ids) + remaining[index:]
    organization.undo(m.library, m.profile_id)
    assert m.library.rows(m.profile_id) == before
    assert original == {str(p): p.read_bytes() for p in source.rglob("*") if p.is_file()}


def test_selection_is_frozen_and_group_drop_is_local(drag):
    w, _ = drag
    m = w.manager
    gesture = mod_drag.begin(w, next(iter(w._selected_mods)))
    w._selected_mods.clear()
    mod_drag.drop(w, gesture, group="Visual/Custom")
    assert all(row.group == "Visual/Custom" for row in m.library.rows(m.profile_id) if row.id in gesture.mod_ids)
    assert m.library.rows(m.library.profiles()[1].id) == m.rows


@pytest.mark.parametrize("change", ["profile", "cached", "database", "runtime", "busy"])
def test_stale_context_preserves_changes(drag, change):
    w, _ = drag
    m = w.manager
    gesture = mod_drag.begin(w, next(iter(w._selected_mods)))
    if change == "profile":
        m.profile_id = m.library.profiles()[1].id
    elif change == "cached":
        m.rows = []
    elif change == "database":
        with m.library._db() as db:
            db.execute("UPDATE entries SET enabled=0 WHERE profile_id=?", (m.profile_id,))
    else:
        setattr(m, "runtime_active" if change == "runtime" else "busy", True)
    before = m.library.rows(m.profile_id)
    with pytest.raises(ManagerError):
        mod_drag.drop(w, gesture, group="Unexpected")
    assert m.library.rows(m.profile_id) == before


def test_drag_commit_cancellation_and_failed_history_roll_back(drag, monkeypatch):
    w, _ = drag
    m = w.manager
    gesture = mod_drag.begin(w, next(iter(w._selected_mods)))
    cancel = threading.Event()
    def stop():
        cancel.set()
    monkeypatch.setattr(organization, "_checkpoint", stop)
    with pytest.raises(Cancelled):
        organization.bulk(m.library, m.profile_id, gesture.mod_ids, group="Cancelled", expected=gesture.expected, cancel=cancel)
    assert m.library.rows(m.profile_id) == m.rows
    def fail():
        raise OSError("generated history failure")
    monkeypatch.setattr(organization, "_checkpoint", fail)
    with pytest.raises(OSError):
        mod_drag.drop(w, gesture, group="Failed")
    assert m.library.rows(m.profile_id) == m.rows
