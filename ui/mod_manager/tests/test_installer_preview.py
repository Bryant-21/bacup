from contextlib import closing
import threading

from PIL import Image
import pytest

from creation_lib.mod_manager.fomod import Fomod
from creation_lib.mod_manager.installer_preview import image, metadata
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_fomod import group_xml


def test_module_option_artwork_metadata_and_bounded_worker_decode(tmp_path):
    root = tmp_path / "package"
    root.mkdir()
    path = root / "preview.png"
    Image.new("RGB", (2048, 1024), (210, 160, 30)).save(path)
    before = path.read_bytes()
    xml = group_xml("SelectAny").replace(b"</moduleName>", b'</moduleName><moduleImage path="preview.png" showImage="true"/>').replace(b'<plugin name="Option 0">', b'<plugin name="Option 0"><image path="preview.png"/>')
    engine = Fomod(xml)
    assert engine.image == "preview.png" and engine.evaluate().steps[0].groups[0].options[0].image == "preview.png"
    assert not Fomod(xml.replace(b'showImage="true"', b'showImage="false"')).image
    with closing(Package(root)) as package:
        pixels = image(package, engine.image)
        assert pixels.shape == (512, 1024, 3) and tuple(pixels[0, 0]) == (210, 160, 30)
        assert path.read_bytes() == before
        cancel = threading.Event()
        cancel.set()
        with pytest.raises(Cancelled):
            image(package, "preview.png", cancel)
        with pytest.raises(ManagerError, match="existing package member"):
            image(package, "missing.png")
        with pytest.raises(ManagerError, match="Unsafe"):
            image(package, "../outside.png")


@pytest.mark.parametrize("fault", ["malformed", "oversized", "stale", "huge-pixels", "image-traversal", "bad-boolean"])
def test_invalid_artwork_is_bounded_and_never_changes_payloads(tmp_path, fault):
    root = tmp_path / "package"
    root.mkdir()
    path = root / "preview.png"
    if fault == "malformed":
        path.write_bytes(b"generated invalid image")
    else:
        Image.new("RGB", (4001, 4000) if fault == "huge-pixels" else (10, 10)).save(path)
    if fault == "oversized":
        path.write_bytes(b"x" * (8 * 1024**2 + 1))
    before = path.read_bytes()
    if fault in {"image-traversal", "bad-boolean"}:
        xml = group_xml("SelectAny").replace(b"</moduleName>", b'</moduleName><moduleImage path="../outside.png"/>' if fault == "image-traversal" else b'</moduleName><moduleImage path="preview.png" showImage="yes"/>')
        with pytest.raises(ManagerError):
            Fomod(xml)
    else:
        with closing(Package(root)) as package:
            if fault == "stale":
                path.write_bytes(b"changed after package review")
                before = path.read_bytes()
            with pytest.raises(ManagerError):
                image(package, "preview.png")
    assert path.read_bytes() == before


@pytest.mark.parametrize("encoding", ["utf-8", "utf-16"])
def test_optional_info_xml_is_bounded_plain_metadata_with_immutable_sources(tmp_path, encoding):
    root = tmp_path / "package"
    root.joinpath("fomod").mkdir(parents=True)
    text = '<fomod><Name>Generated Name</Name><Author>Fixture Author</Author><Version>1.2</Version><Id>123</Id><Website>https://example.invalid/fixture</Website><Description>Inert fixture information</Description></fomod>'
    path = root / "fomod" / "info.xml"
    path.write_bytes(text.encode(encoding))
    original = path.read_bytes()
    with closing(Package(root)) as package:
        data = metadata(package)
    assert data["Name"] == "Generated Name" and data["Id"] == "123" and data["Author"] == "Fixture Author"
    assert path.read_bytes() == original


@pytest.mark.parametrize("text", ["<bad/>", '<!DOCTYPE fomod [<!ENTITY unsafe SYSTEM "file:///fixture">]><fomod/>', '<fomod><Name>A</Name><Name>B</Name></fomod>', '<fomod><Name><script>inert</script></Name></fomod>'])
def test_invalid_optional_information_never_runs_external_content(tmp_path, text):
    root = tmp_path / "package"
    root.joinpath("fomod").mkdir(parents=True)
    path = root / "fomod" / "info.xml"
    path.write_text(text)
    with closing(Package(root)) as package:
        with pytest.raises(ManagerError):
            metadata(package)
    assert path.read_text() == text
