import threading
import uuid

import pytest

from creation_lib.mod_manager.nexus_auth import CredentialStore as WindowsCredentials, authenticate
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.nexus import NexusClient, NexusError
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_nexus import Transport


class Store:
    def __init__(self, key=""):
        self.key = key

    def read(self):
        return self.key

    def write(self, key):
        self.key = key

    def delete(self):
        self.key = ""


def test_windows_credentials_roundtrip_uses_only_disposable_owned_target():
    store = WindowsCredentials("ModBox21/Nexus/test-" + uuid.uuid4().hex)
    try:
        assert store.read() == ""
        store.write("generated-disposable-credential")
        assert store.read() == "generated-disposable-credential"
        store.delete()
        assert store.read() == ""
        with pytest.raises(ManagerError):
            store.write("bad\nkey")
    finally:
        store.delete()


def test_validate_before_persist_and_restore_without_key_in_result():
    store = Store()
    def factory(key):
        return NexusClient(key, transport=Transport())
    client = authenticate("fixture-key", True, store=store, factory=factory)
    assert store.key == "fixture-key" and client.user["user_id"] == 42
    restored = authenticate(store=store, factory=factory)
    assert restored.user == client.user and "fixture-key" not in repr(restored.user)


def test_bad_login_and_cancel_preserve_previous_saved_credential():
    store = Store("previous")
    def invalid(key):
        client = NexusClient(key, transport=Transport())
        client.connect = lambda: (_ for _ in ()).throw(NexusError("Unauthorized"))
        return client
    with pytest.raises(NexusError):
        authenticate("bad", True, store=store, factory=invalid)
    assert store.key == "previous"
    cancel = threading.Event()
    def factory(key):
        client = NexusClient(key, transport=Transport())
        connect = client.connect
        def cancelled_connect():
            connect()
            cancel.set()
        client.connect = cancelled_connect
        return client
    with pytest.raises(Cancelled):
        authenticate("new", True, store=store, factory=factory, cancel=cancel)
    assert store.key == "previous"


def test_startup_reconnect_once_and_disconnect_forgets_saved_credential(tmp_path):
    manager = ManagerController()
    try:
        manager.library = Library(tmp_path / "library", create=True)
        manager.credential_store = Store("fixture-key")
        calls = []
        manager.connect_nexus = lambda: calls.append("reconnect")
        manager.tick_updates()
        manager.tick_updates()
        assert calls == ["reconnect"]
        manager.disconnect_nexus()
        assert manager.credential_store.read() == ""
        manager.tick_updates()
        assert calls == ["reconnect"]
    finally:
        manager.cleanup()
