"""Normal XML header semantics and optional art failures."""
import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.fomod import Fomod
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import ManagerError
from .test_controller import finish


@pytest.mark.parametrize("position", ["Left", "Right", "RightOfImage"])
@pytest.mark.parametrize("colour,rgba", [("FF8040", (1, 128/255, 64/255, 1)), ("80FF8040", (1, 128/255, 64/255, 128/255))])
def test_title_position_and_rgb_argb_values(position, colour, rgba):
    engine = Fomod(f'<config><moduleName position="{position}" colour="{colour}">Fixture</moduleName></config>'.encode())
    assert engine.decoration.position == position and engine.decoration.colour == rgba


@pytest.mark.parametrize("attributes,image,fade,height", [
    ('', 'fomod/screenshot.png', True, -1),
    ('path="art.png" showFade="false" height="120"', 'art.png', False, 120),
    ('path="art.png" showImage="false"', '', True, -1),
    ('path="art.png" height="10000"', 'art.png', True, 10000),
])
def test_module_image_defaults_visibility_and_authored_height(attributes, image, fade, height):
    engine = Fomod(f'<config><moduleName>Fixture</moduleName><moduleImage {attributes}/></config>'.encode())
    assert (engine.decoration.image, engine.decoration.fade, engine.decoration.height) == (image, fade, height)


@pytest.mark.parametrize("title,image", [
    ('position="Centre"', ''), ('colour="red"', ''), ('colour="12345"', ''),
    ('', '<moduleImage height="NaN"/>'), ('', '<moduleImage path="../escape"/>'),
])
def test_invalid_declarations_fail_before_installer_payload_review(title, image):
    with pytest.raises(ManagerError):
        Fomod(f'<config><moduleName {title}>Fixture</moduleName>{image}</config>'.encode())


@pytest.mark.parametrize("art", ["absent.png", "invalid.png"])
def test_optional_art_failure_does_not_block_valid_installer_plan(tmp_path, art):
    m = ManagerController()
    try:
        m.library = Library(tmp_path / "library", create=True)
        m.profile_id = m.library.create_profile("Local")
        m.refresh()
        root = tmp_path / "package"
        (root / "fomod").mkdir(parents=True)
        (root / "payload.txt").write_bytes(b"generated")
        (root / "invalid.png").write_bytes(b"invalid artwork")
        (root / "fomod/ModuleConfig.xml").write_text(f'<config><moduleName>Fixture</moduleName><moduleImage path="{art}"/><requiredInstallFiles><file source="payload.txt" destination="payload.txt"/></requiredInstallFiles></config>')
        m.prepare_install(root)
        finish(m)
        assert m.prepared.header_image is None and m.prepared.metadata["Header artwork"]
        m.review_install()
        finish(m)
        assert m.install_plan == {"payload.txt": "payload.txt"}
        assert (root / "payload.txt").read_bytes() == b"generated"
    finally:
        m.cleanup()
