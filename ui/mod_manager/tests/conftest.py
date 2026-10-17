import pytest


@pytest.fixture(autouse=True)
def isolated_manager_data(tmp_path, monkeypatch):
    from creation_lib.mod_manager import nexus_auth
    class MemoryCredentials:
        def __init__(self):
            self.key = ""

        def read(self):
            return self.key

        def write(self, key):
            self.key = key

        def delete(self):
            self.key = ""
    monkeypatch.setattr(nexus_auth, "CredentialStore", MemoryCredentials)
    from app import paths
    from ui.toolkit import app_paths
    root = tmp_path / "app-data"
    monkeypatch.setattr(paths, "get_db_dir", lambda: root)
    monkeypatch.setattr(app_paths, "get_db_dir", lambda: root)
