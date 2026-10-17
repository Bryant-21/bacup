"""Project-owned authored-stage rendering through the native SWF boundary."""
from pathlib import Path

from creation_lib.swf import native_runtime


def render_stage(movie,frame,owner,settings,cancel):
    if cancel.is_set():
        raise RuntimeError('Stage render cancelled')
    fonts = []
    if settings is not None:
        paths = settings.get_game_paths(settings.active_game)
        for root in (paths.get('extracted_dir',''),):
            if root:
                font_path = Path(root)/'Interface'/'fonts_en.swf'
                if font_path.is_file() and font_path.stat().st_size < 32*1024*1024:
                    fonts.append(font_path.read_bytes())
                    break
    result = native_runtime.render_frame_image(movie,frame,owner,fonts)
    if cancel.is_set():
        raise RuntimeError('Stage render cancelled')
    from io import BytesIO
    from PIL import Image
    with Image.open(BytesIO(result['png'])) as image:
        image = image.convert('RGBA')
        return image.size,image.tobytes(),result['bounds'],result['warnings']
