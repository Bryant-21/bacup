"""Explicit native acceptance probe; only generated files and Python are launched.

Not part of automatic pytest: requires a user-selected existing USVFS bundle.
No MO2 program, game, real profile, or original mod payload is run or modified.
"""
from dataclasses import replace
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from creation_lib.mod_manager.launcher import fingerprint
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.runtime import RuntimePlan, bundle, profile_signature
from .fixtures import make_mo2


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    runtime_bundle = bundle(str(args.bundle))
    original_bundle = {name: (path.stat().st_size, path.stat().st_mtime_ns) for path in runtime_bundle.iterdir() if path.is_file() for name in [path.name]}
    with tempfile.TemporaryDirectory(prefix="mod-manager-native-fixture-") as temporary:
        root = Path(temporary)
        source = make_mo2(root)
        (source / "mods" / "Disabled" / "disabled-only.txt").write_text("disabled fixture")
        hidden = source / "mods" / "Override" / "folder.mohidden"
        hidden.mkdir()
        (hidden / "secret.txt").write_text("hidden fixture")
        library = Library(root / "library", create=True)
        library.import_mo2(inspect_instance(source))
        profile_id = library.profiles()[0].id
        game = root / "game"
        (game / "Data").mkdir(parents=True)
        user = root / "user"
        (user / "plugins").mkdir(parents=True)
        (user / "ini" / "Saves").mkdir(parents=True)
        (user / "plugins" / "plugins.txt").write_text("*Global.esp\n")
        (user / "ini" / "Fallout4.ini").write_text("[Fixture]\nvalue=GLOBAL\n")
        (user / "ini" / "Saves" / "global.sav").write_bytes(b"global fixture")
        # Use the normal planner with a generated PE image, then substitute the
        # harmless Python probe only in this explicit fixture utility.
        from .test_runtime import pe
        from creation_lib.mod_manager.launcher import Executable
        from creation_lib.mod_manager.runtime import prepare
        fake_bundle = root / "fake-bundle"
        fake_bundle.mkdir()
        for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
            pe(fake_bundle / name, machine)
        binary = pe(game / "f4se_loader.exe")
        pe(game / "Fallout4.exe")
        executable = Executable("fixture", "Harmless native probe", str(binary))
        plan = prepare(library, profile_id, executable, str(game), str(fake_bundle), user_targets=(user / "plugins", user / "ini"), isolate_documents=True)
        result = root / "probe-result.json"
        parameters = {"data": str(game / "Data"), "plugins": str(user / "plugins" / "plugins.txt"),
                      "ini": str(user / "ini" / "Fallout4.ini"), "saves": str(user / "ini" / plan.save_subpath), "result": str(result)}
        code = r'''
import configparser,json,pathlib,subprocess,sys
p=json.loads(sys.argv[1]); data=pathlib.Path(p['data']); save=pathlib.Path(p['saves'])
assert (data/'textures/shared.txt').read_bytes()==b'override'
assert not (data/'hidden.mohidden').exists()
assert not (data/'folder.mohidden/secret.txt').exists()
assert not (data/'disabled-only.txt').exists()
assert pathlib.Path(p['plugins']).read_text().startswith('*Fixture.esp')
ini=configparser.ConfigParser(); ini.read(p['ini']); assert ini['Fixture']['value']=='unchanged'
assert ini['General']['SLocalSavePath'].startswith('__ModBox21Saves_')
assert not (save/'global.sav').exists()
(save/'local.sav').write_bytes(b'local fixture')
(data/'new-output.txt').write_bytes(b'owned output')
child_code="import pathlib,sys; p=pathlib.Path(sys.argv[1]); assert (p/'textures/shared.txt').read_bytes()==b'override'; (p/'child-output.txt').write_bytes(b'child fixture')"
child=subprocess.run([sys.executable,'-c',child_code,str(data)],capture_output=True,timeout=15)
assert child.returncode==0,child.stderr.decode(errors='replace')
(data/'textures/shared.txt').write_bytes(b'existing mapped file is writable')
pathlib.Path(p['result']).write_text(json.dumps({'priority':True,'hidden':True,'disabled':True,'plugins':True,'ini':True,'save_isolation':True,'new_output':True,'child_inheritance':True,'existing_source_write':True}))
subprocess.Popen([sys.executable,'-c',"import pathlib,sys,time; time.sleep(.4); (pathlib.Path(sys.argv[1])/'late-child.txt').write_bytes(b'late child')",str(data)])
'''
        launch = replace(plan.launch, argv=(sys.executable, "-c", code, json.dumps(parameters)),
                         executable=replace(plan.launch.executable, binary=sys.executable), signature=fingerprint(Path(sys.executable)))
        plan = replace(plan, launch=launch, bundle_root=str(runtime_bundle))
        assert isinstance(plan, RuntimePlan) and plan.profile_signature == profile_signature(library, profile_id)
        session = library.root / "runtime" / "sessions" / "native-fixture"
        session.mkdir(parents=True)
        request = session / "request.json"
        request.write_text(json.dumps(plan.record()))
        from creation_lib.mod_manager.runtime import authorize_helper, helper_command
        from creation_lib.mod_manager.process_identity import from_handle, kernel_api, process_state, terminate_owned
        child = subprocess.Popen(helper_command(request),
                                 cwd=str(Path(__file__).resolve().parents[3]), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 creationflags=subprocess.CREATE_NO_WINDOW)
        child_identity = from_handle(child.pid, child._handle)
        try:
            authorize_helper(child, session, profile_id)
            _, stderr = child.communicate(timeout=35)
        finally:
            if child.poll() is None:
                terminate_owned(child, child_identity)
            child.wait(timeout=10)
        status_path = session / "status.json"
        status = json.loads(status_path.read_text()) if status_path.exists() else {"error": stderr.decode(errors="replace")}
        assert child.returncode == 0 and status.get("exit_code") == 0 and result.is_file(), {"status": status, "owner": json.loads((session / "owner.json").read_text())}
        evidence = json.loads(result.read_text())
        profile_root = library.root / "runtime" / "profiles" / profile_id
        assert (profile_root / "overwrite" / "new-output.txt").read_bytes() == b"owned output"
        assert (profile_root / "overwrite" / "child-output.txt").read_bytes() == b"child fixture"
        assert (profile_root / "overwrite" / "late-child.txt").read_bytes() == b"late child"
        evidence["child_after_parent_exit"] = True
        assert (profile_root / "saves" / "local.sav").read_bytes() == b"local fixture"
        assert not any((game / "Data").iterdir())
        assert (user / "plugins" / "plugins.txt").read_text() == "*Global.esp\n"
        assert (user / "ini" / "Fallout4.ini").read_text().endswith("value=GLOBAL\n")
        assert (user / "ini" / "Saves" / "global.sav").read_bytes() == b"global fixture"
        assert (source / "mods" / "Override" / "textures" / "shared.txt").read_bytes() == b"existing mapped file is writable"
        assert (source / "mods" / "Base" / "textures" / "shared.txt").read_bytes() == b"base"
        # Kill only a helper we created for a sleeping disposable Python tree.
        # The atomically assigned job must remove both fixture descendants.
        crash_marker = root / "crash-pids.json"
        crash_code = "import json,os,pathlib,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(json.dumps([os.getpid(),child.pid])); time.sleep(30)"
        files = tuple((name, source, fingerprint(Path(source))) for name, source, _ in plan.files)
        crash_plan = replace(plan, files=files, launch=replace(plan.launch, argv=(sys.executable, "-c", crash_code, str(crash_marker))))
        crash_session = library.root / "runtime" / "sessions" / "native-crash-fixture"
        crash_session.mkdir()
        crash_request = crash_session / "request.json"
        crash_request.write_text(json.dumps(crash_plan.record()))
        helper = subprocess.Popen(helper_command(crash_request),
                                  cwd=str(Path(__file__).resolve().parents[3]), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                  creationflags=subprocess.CREATE_NO_WINDOW)
        from creation_lib.mod_manager.runtime import active_sessions
        helper_identity = from_handle(helper.pid, helper._handle)
        try:
            authorize_helper(helper, crash_session, profile_id)
            deadline = time.monotonic() + 15
            while not crash_marker.exists() and helper.poll() is None and time.monotonic() < deadline:
                time.sleep(.05)
            assert crash_marker.exists(), json.loads((crash_session / "status.json").read_text())
            fixture_pids = json.loads(crash_marker.read_text())
            identities = []
            kernel = kernel_api()
            for pid in fixture_pids:
                handle = kernel.OpenProcess(0x1000 | 0x100000, False, pid)
                assert handle
                try:
                    identities.append(from_handle(pid, handle))
                finally:
                    kernel.CloseHandle(handle)
            assert all(process_state(value) == "alive" for value in identities) and active_sessions(library.root)
            wrong_identity = {**helper_identity, "created": str(int(helper_identity["created"]) + 1)}
            assert process_state(wrong_identity) == "reused"
            evidence["reused_identity_is_not_owned"] = True
            terminate_owned(helper, helper_identity)
            helper.wait(timeout=10)
            deadline = time.monotonic() + 5
            while any(process_state(value) == "alive" for value in identities) and time.monotonic() < deadline:
                time.sleep(.05)
            assert all(process_state(value) in {"exited", "reused"} for value in identities)
            assert not active_sessions(library.root)
            assert Library(library.root).create_profile("Recovered after native fixture crash")
            evidence["helper_crash_kills_owned_fixture_tree"] = True
            evidence["restart_unlock_after_crash"] = True
            evidence["handle_creation_identities"] = True
        finally:
            if helper.poll() is None:
                terminate_owned(helper, helper_identity)
            helper.wait(timeout=10)
        unchanged = {name: (path.stat().st_size, path.stat().st_mtime_ns) for path in runtime_bundle.iterdir() if path.is_file() for name in [path.name]}
        assert unchanged == original_bundle, "Runtime bundle files changed"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps({"version": "0.5.7.2", "architecture": "x64", "fixture": "Python executable and generated metadata/payloads", "checks": evidence,
                                           "real_game_tested": False, "runtime_bundle_unchanged": True}, indent=2))
        print(json.dumps(evidence))


if __name__ == "__main__":
    main()
