def test_bacup_launcher_constructs_its_workspace_directly(monkeypatch):
    import bacup_ui.__main__ as launcher

    events = []

    class FakeSettings:
        def __init__(self, variant_id):
            self.variant_id = variant_id
            self.active_workspace = None

    class FakeWorkspace:
        def __init__(self, toolkit_settings):
            events.append(("workspace", toolkit_settings.variant_id))

    class FakeApp:
        def __init__(self, workspaces, settings, *, launch_path, app_variant):
            events.append(
                (
                    "app",
                    len(workspaces),
                    settings.active_workspace,
                    launch_path,
                    app_variant.exe_name,
                )
            )

        def run(self):
            events.append(("run",))

    monkeypatch.setattr(launcher, "ToolkitSettings", FakeSettings)
    monkeypatch.setattr(launcher, "AppalachiaWorkspace", FakeWorkspace)
    monkeypatch.setattr(launcher, "ToolkitApp", FakeApp)
    monkeypatch.setattr(launcher, "_set_taskbar_identity", lambda: None)
    monkeypatch.setattr(
        launcher, "_run_bacup_project_setup", lambda _settings: (False, True)
    )

    launcher.run_bacup("input.ba2")

    assert events == [
        ("workspace", "appalachia"),
        ("app", 1, "appalachia", "input.ba2", "BACUP"),
        ("run",),
    ]
