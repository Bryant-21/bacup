"""End-to-end project filespace/helper acceptance against disposable Python fixtures."""
from dataclasses import replace
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import time

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.runtime import prepare, helper_command, authorize_helper, active_sessions
from creation_lib.mod_manager.launcher import Executable, fingerprint
from creation_lib.mod_manager.process_identity import from_handle, terminate_owned, process_state, kernel_api
from .fixtures import make_mo2
from .test_runtime import pe


def launch_fixture(library, plan, name, *, cancelled=False, frozen_app=None):
    session = Path(plan.library_root) / "runtime" / "sessions" / name
    session.mkdir(parents=True)
    request = session / "request.json"
    request.write_text(json.dumps(plan.record()))
    if cancelled:
        (session / "cancel").touch()
    from creation_lib.mod_manager.runtime_entry import FLAG
    command = [str(frozen_app), FLAG, str(request)] if frozen_app else helper_command(request)
    environment = dict(os.environ)
    if frozen_app:
        environment.pop("MOD_ROOT", None)
    helper = subprocess.Popen(command, cwd=str(Path(frozen_app).parent if frozen_app else Path(__file__).resolve().parents[3]), env=environment,
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                              creationflags=subprocess.CREATE_NO_WINDOW)
    identity = from_handle(helper.pid, helper._handle)
    authorize_helper(helper, session, plan.launch.profile_id)
    return helper, identity, session


def run(frozen_app=None):
    with tempfile.TemporaryDirectory(prefix="modbox-project-runtime-") as temporary:
        root = Path(temporary)
        source = make_mo2(root)
        (source / "mods" / "Disabled" / "disabled-only.txt").write_bytes(b"disabled")
        library = Library(root / "library", create=True)
        library.import_mo2(inspect_instance(source))
        profile = library.profiles()[0].id
        game, user = root / "game", root / "user"
        (game / "Data").mkdir(parents=True)
        for part in ("plugins", "ini/Saves"):
            (user / part).mkdir(parents=True)
        (user / "plugins/plugins.txt").write_bytes(b"*Global.esp\n")
        (user / "ini/Fallout4.ini").write_text("[Fixture]\nvalue=GLOBAL\n")
        (user / "ini/Saves/global.sav").write_bytes(b"global")
        pe(game / "Fallout4.exe")
        plan = prepare(library, profile, Executable("fixture", "Python fixture", str(game / "Fallout4.exe")), str(game), "",
                       user_targets=(user / "plugins", user / "ini"), isolate_documents=True)
        if frozen_app:
            plan = replace(plan, bundle_root=str(Path(frozen_app).parent / "_internal/creation_lib/mod_manager/_runtime/project-vfs"))
        originals = {path: path.read_bytes() for folder in (source, game, user) for path in folder.rglob("*") if path.is_file()}
        parameters = {"data": str(game / "Data"), "plugins": str(user / "plugins/plugins.txt"), "ini": str(user / "ini/Fallout4.ini"),
                      "saves": str(user / "ini" / plan.save_subpath), "result": str(root / "result.json")}
        code = r'''
import configparser,json,pathlib,subprocess,sys,os,tempfile
p=json.loads(sys.argv[1]); data=pathlib.Path(p['data']); saves=pathlib.Path(p['saves'])
assert (data/'textures/shared.txt').read_bytes()==b'override'
assert not (data/'hidden.mohidden').exists() and not (data/'disabled-only.txt').exists()
assert pathlib.Path(p['plugins']).read_text().startswith('*Fixture.esp')
ini=configparser.ConfigParser(); ini.read(p['ini']); assert ini['Fixture']['value']=='unchanged'
assert ini['General']['SLocalSavePath'].startswith('__ModBox21Saves_')
owned_ini=pathlib.Path(p['ini']); owned_ini.write_text(owned_ini.read_text()+'\n[OwnedFixture]\nvalue=local write\n')
assert 'local write' in owned_ini.read_text()
for projected_path in (owned_ini.parent/'unexpected.ini.tmp',pathlib.Path(p['plugins']).parent/'unexpected.plugins.tmp',owned_ini.parent/'Saves/global.sav'):
    projected_path.write_bytes(b'owned projected write')
    assert projected_path.read_bytes()==b'owned projected write'
assert not (saves/'global.sav').exists()
(saves/'local.sav').write_bytes(b'local')
(data/'new-output.txt').write_bytes(b'owned')
(data/'textures/shared.txt').write_bytes(b'protected COW write')
assert (data/'textures/shared.txt').read_bytes()==b'protected COW write'
temp=pathlib.Path(tempfile.gettempdir()); atomic=temp/'fixture-atomic.new'
atomic.write_bytes(b'atomic helper replacement'); os.replace(atomic,data/'textures/shared.txt')
assert (data/'textures/shared.txt').read_bytes()==b'atomic helper replacement' and not atomic.exists()
(data/'new-output.txt').unlink(); assert not (data/'new-output.txt').exists()
(data/'new-output.txt').write_bytes(b'owned')
child_code="import pathlib,sys; p=pathlib.Path(sys.argv[1]); assert (p/'textures/shared.txt').read_bytes()==b'protected COW write'; (p/'child-output.txt').write_bytes(b'child')"
child_code=child_code.replace('protected COW write','atomic helper replacement')
child=subprocess.run([sys.executable,'-c',child_code,str(data)],capture_output=True,timeout=15)
assert child.returncode==0,child.stderr.decode(errors='replace')
pathlib.Path(p['result']).write_text(json.dumps({'priority':True,'hidden_disabled':True,'plugins':True,'inis':True,'owned_configuration_writes':True,'user_directory_writes_owned':True,'save_isolation':True,'new_output':True,'cow_source_protection':True,'python_child_inheritance':True,'owned_temp_atomic_replace':True,'helper_delete_recreate':True}))
subprocess.Popen([sys.executable,'-c',"import pathlib,sys,time; time.sleep(.3); (pathlib.Path(sys.argv[1])/'late.txt').write_bytes(b'late')",str(data)])
'''
        code = "import json,pathlib,sys,traceback\ntry:\n" + textwrap.indent(code, "    ") + "\nexcept BaseException:\n    pathlib.Path(json.loads(sys.argv[1])['result']+'.error').write_text(traceback.format_exc())\n    raise\n"
        python = Path(getattr(sys, "_base_executable", None) or sys.executable)
        plan = replace(plan, launch=replace(plan.launch, argv=(str(python), "-c", code, json.dumps(parameters)),
                                           executable=replace(plan.launch.executable, binary=str(python)), signature=fingerprint(python)))
        helper, identity, session = launch_fixture(library, plan, "native-fixture", frozen_app=frozen_app)
        try:
            helper.wait(timeout=45)
            status = json.loads((session / "status.json").read_text())
            error = root / "result.json.error"
            assert helper.returncode == 0 and status.get("exit_code") == 0 and (root / "result.json").exists(), (status, error.read_text() if error.exists() else "No fixture error report")
        finally:
            if helper.poll() is None:
                terminate_owned(helper, identity)
            helper.wait(timeout=10)
        result = json.loads((root / "result.json").read_text())
        owned = session / "output"
        assert Path(status["output_root"]) == owned
        assert not (library.root / "runtime/profiles" / profile / "overwrite/textures/shared.txt").exists()
        assert (owned / "overwrite/textures/shared.txt").read_bytes() == b"atomic helper replacement"
        assert not (owned / "temp/fixture-atomic.new").exists()
        assert (owned / "overwrite/late.txt").read_bytes() == b"late"
        assert (owned / "saves/local.sav").read_bytes() == b"local"
        assert (owned / "ini/unexpected.ini.tmp").read_bytes() == b"owned projected write"
        assert (owned / "plugins/unexpected.plugins.tmp").read_bytes() == b"owned projected write"
        assert (owned / "ini/Saves/global.sav").read_bytes() == b"owned projected write"
        assert not (user / "ini/unexpected.ini.tmp").exists() and not (user / "plugins/unexpected.plugins.tmp").exists()
        assert all(path.read_bytes() == data for path, data in originals.items())
        assert not list((game / "Data").iterdir())
        result.update(source_game_user_unchanged=True, child_after_parent_exit=True, process_creation_identities=True, session_output_isolated=True)
        assert not active_sessions(library.root)
        shared_code = r'''
import configparser, pathlib, sys
documents = pathlib.Path(sys.argv[1])
ini = configparser.ConfigParser(); ini.read(documents / 'Fallout4.ini')
assert ini['General']['SLocalSavePath'] == 'Saves\\'
assert (documents / 'Saves/global.sav').read_bytes() == b'global'
(documents / 'Saves/shared.sav').write_bytes(b'shared save')
(documents / 'F4SE').mkdir(exist_ok=True)
(documents / 'F4SE/shared.log').write_bytes(b'shared log')
(documents / 'OtherTool').mkdir(exist_ok=True)
(documents / 'OtherTool/settings.txt').write_bytes(b'shared settings')
(documents / 'Fallout4.ini').write_text('[Fixture]\nvalue=private configuration\n')
'''
        shared_code = "import pathlib,sys,traceback\ntry:\n" + textwrap.indent(shared_code, "    ") + "\nexcept BaseException:\n    pathlib.Path(sys.argv[2]).write_text(traceback.format_exc())\n    raise\n"
        shared = replace(plan, isolate_documents=False,
                         protected_roots=tuple(path for path in plan.protected_roots if Path(path) != user / "ini"),
                         launch=replace(plan.launch, argv=(str(python), "-c", shared_code, str(user / "ini"), str(root / "shared.error"))))
        helper, identity, shared_session = launch_fixture(library, shared, "shared-documents", frozen_app=frozen_app)
        try:
            helper.wait(timeout=30)
            shared_status = json.loads((shared_session / "status.json").read_text())
            assert helper.returncode == 0 and shared_status["exit_code"] == 0, (shared_status, (root / "shared.error").read_text() if (root / "shared.error").exists() else "No exception")
            assert (user / "ini/Saves/shared.sav").read_bytes() == b"shared save"
            assert (user / "ini/F4SE/shared.log").read_bytes() == b"shared log"
            assert (user / "ini/OtherTool/settings.txt").read_bytes() == b"shared settings"
            assert (user / "ini/Fallout4.ini").read_bytes() == originals[user / "ini/Fallout4.ini"]
            assert "private configuration" in (shared_session / "output/ini/Fallout4.ini").read_text()
            result["shared_documents_saves_logs_and_other_folders"] = True
        finally:
            if helper.poll() is None:
                terminate_owned(helper, identity)
            helper.wait(timeout=10)
        cancelled_marker = root / "unexpected-cancelled-child.txt"
        child_failure = "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c','import time; time.sleep(1); raise SystemExit(7)']); time.sleep(.5)"
        failed_plan = replace(plan, launch=replace(plan.launch, argv=(str(python), "-c", child_failure)))
        helper, identity, failed_session = launch_fixture(library, failed_plan, "child-exit-failure", frozen_app=frozen_app)
        try:
            helper.wait(timeout=30)
            failed_status = json.loads((failed_session / "status.json").read_text())
            assert helper.returncode == 1 and failed_status["state"] == "failed" and failed_status["exit_code"] == 7, failed_status
            result["child_failure_survives_successful_loader_exit"] = True
        finally:
            if helper.poll() is None:
                terminate_owned(helper, identity)
            helper.wait(timeout=10)
        cancelled_plan = replace(plan, launch=replace(plan.launch, argv=(str(python), "-c", "import pathlib,sys; pathlib.Path(sys.argv[1]).write_text('unexpected')", str(cancelled_marker))))
        helper, identity, session = launch_fixture(library, cancelled_plan, "cancelled-fixture", cancelled=True, frozen_app=frozen_app)
        try:
            helper.wait(timeout=15)
            status = json.loads((session / "status.json").read_text())
            assert status["state"] == "cancelled" and not status.get("process_identity") and not cancelled_marker.exists(), status
            assert not active_sessions(library.root)
            result["precreation_cancel_and_restart_unlock"] = True
        finally:
            if helper.poll() is None:
                terminate_owned(helper, identity)
            helper.wait(timeout=10)
        marker = root / "crash.json"
        crash_code = "import json,os,pathlib,subprocess,sys,time; c=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(json.dumps([os.getpid(),c.pid])); time.sleep(30)"
        crash_plan = replace(plan, launch=replace(plan.launch, argv=(str(python), "-c", crash_code, str(marker))))
        helper, identity, session = launch_fixture(library, crash_plan, "crash-fixture", frozen_app=frozen_app)
        try:
            deadline = time.monotonic() + 20
            while not marker.exists() and helper.poll() is None and time.monotonic() < deadline:
                time.sleep(.03)
            assert marker.exists(), json.loads((session / "status.json").read_text())
            kernel = kernel_api()
            processes = []
            for pid in json.loads(marker.read_text()):
                handle = kernel.OpenProcess(0x1000 | 0x100000, False, pid)
                assert handle
                processes.append(from_handle(pid, handle))
                kernel.CloseHandle(handle)
            assert all(process_state(value) == "alive" for value in processes)
            terminate_owned(helper, identity)
            helper.wait(timeout=10)
            deadline = time.monotonic() + 5
            while any(process_state(value) == "alive" for value in processes) and time.monotonic() < deadline:
                time.sleep(.03)
            assert all(process_state(value) in {"exited", "reused"} for value in processes)
            assert not active_sessions(library.root)
            result.update(helper_crash_kills_owned_tree=True, restart_unlock_after_crash=True)
        finally:
            if helper.poll() is None:
                terminate_owned(helper, identity)
            helper.wait(timeout=10)
        result["backend"] = plan.runtime
        result["frozen_helper"] = bool(frozen_app)
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--frozen-app", type=Path)
    args = parser.parse_args()
    result = run(args.frozen_app)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
