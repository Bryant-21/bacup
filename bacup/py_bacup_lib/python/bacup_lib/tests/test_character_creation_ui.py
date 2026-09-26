import struct
from bacup_lib.character_creation_ui import character_translation_keys


def test_character_translations_include_html_edit_text_and_nested_sprites():
    def tag(code, payload):
        return (struct.pack('<H', code << 6 | len(payload)) if len(payload) < 63 else
                struct.pack('<HI', code << 6 | 63, len(payload))) + payload
    # DefineEditText: bounds, HasText+HTML, variable, initial text. No FO76 bytes.
    text = b'\x01\x00\x08\x00\x80\x02\x00<p><b>$BODY_MUSCULAR</b></p>\x00'
    nested = struct.pack('<HH', 2, 1) + tag(37, text) + tag(0, b'')
    body = b'\x08\x00\x00\x18\x01\x00' + tag(39, nested) + tag(1, b'') + tag(0, b'')
    movie = b'FWS\x0a' + struct.pack('<I', 8 + len(body)) + body
    assert '$BODY_MUSCULAR' in character_translation_keys(movie)
