from __future__ import annotations

import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image

from ui.tools.image.image_utils import (
    ImageUtilsFormat,
    convert_image_to_ico,
    convert_image_to_svg,
    convert_images,
)


def _write_test_image(path: Path, size: tuple[int, int] = (32, 24)) -> None:
    image = Image.new("RGBA", size, (0, 0, 0, 0))
    for x in range(4, size[0] - 4):
        for y in range(4, size[1] - 4):
            image.putpixel((x, y), (255, 0, 0, 255))
    image.save(path)


def test_convert_image_to_ico_uses_requested_size(tmp_path):
    source = tmp_path / "source.png"
    output = tmp_path / "icon.ico"
    _write_test_image(source, size=(48, 48))

    written = convert_image_to_ico(source, output, icon_size=32)

    assert written == output
    with Image.open(output) as icon:
        assert icon.size == (32, 32)


def test_convert_image_to_svg_writes_vector_paths(tmp_path):
    source = tmp_path / "source.png"
    output = tmp_path / "shape.svg"
    _write_test_image(source)

    written = convert_image_to_svg(source, output)

    assert written == output
    root = ET.fromstring(output.read_text(encoding="utf-8"))
    assert root.tag.endswith("svg")
    assert root.findall(".//{http://www.w3.org/2000/svg}path")


def test_convert_images_batches_folder_to_output_folder(tmp_path):
    source_dir = tmp_path / "images"
    output_dir = tmp_path / "converted"
    source_dir.mkdir()
    _write_test_image(source_dir / "a.png")
    _write_test_image(source_dir / "b.png")
    (source_dir / "notes.txt").write_text("ignore", encoding="utf-8")

    written = convert_images(
        source_dir,
        target_format=ImageUtilsFormat.ICO,
        output_dir=output_dir,
        icon_size=16,
    )

    assert [path.name for path in written] == ["a.ico", "b.ico"]
    assert all(path.parent == output_dir for path in written)
    for path in written:
        with Image.open(path) as icon:
            assert icon.size == (16, 16)


def test_convert_images_single_image_uses_output_file_over_output_folder(tmp_path):
    source = tmp_path / "source.png"
    output_dir = tmp_path / "ignored"
    output_file = tmp_path / "custom-name.ico"
    _write_test_image(source)

    written = convert_images(
        source,
        target_format=ImageUtilsFormat.ICO,
        output_dir=output_dir,
        output_file=output_file,
        icon_size=24,
    )

    assert written == [output_file]
    assert not output_dir.exists()
    with Image.open(output_file) as icon:
        assert icon.size == (24, 24)
