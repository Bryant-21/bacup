"""Audio tool workspaces -- Audio Extractor, Gun Fire Generator, Laser Beam Generator."""

from creation_lib.ui.widgets.user_guide import UserGuide
from creation_lib.ui.workspaces.voice_browser import VoiceBrowserWorkspace
from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.audio.extractor import AudioExtractorTool
from ui.tools.audio.gun_fire import GunFireTool
from ui.tools.audio.laser_beam import LaserBeamTool


class AudioExtractorWorkspace(ToolWorkspace):
    name = "Audio Extractor"
    icon = "AXT"
    id = "audio_extractor"
    tool_class = AudioExtractorTool


class GunFireWorkspace(ToolWorkspace):
    name = "Gun Fire Generator"
    icon = "GUN"
    id = "gun_fire"
    tool_class = GunFireTool

    def get_user_guide(self) -> UserGuide | None:
        return UserGuide(
            title="Gun Fire Generator User Guide",
            window_id="user_guide_gun_fire",
            body="""# Gun Fire Generator

This turns one good single-shot WAV into auto fire. Give it a few RPMs, pick a shot count, and it writes one WAV per RPM. If there are enough shots, it also writes loop markers.

## Quick setup

1. Pick Source WAV. Must be single shot.
2. Pick Output Dir (optional)
3. Put RPMs in RPMs (CSV), like `450, 600, 750`.
4. Set Shot Count. `8` to `16` is usually enough to hear the loop.
5. Run it. :)

Output names come from the source file. If the name has `single`, the tool swaps it to `auto`; otherwise it adds `_auto`, then appends the RPM.

## Timing and trim

Tail Threshold trims the quiet end of the source shot. Lower keeps more tail. Higher makes the loop tighter, but can chop off useful decay.

Jitter (ms) nudges each shot a little early or late. Keep it small for weapons that should feel mechanical. Set it to `0` for locked timing.

Shot Count is just how many shots get generated. With two or more shots, the tool writes cue and sampler loop markers from the second shot to the last shot.

## Per-shot variation

Pitch Variation adds small pitch drift per shot. Keep it subtle. `0.2` to `0.6` is usually enough.

Gain Variation (dB) changes shot level before the final file gets normalized. If some shots jump out too much, turn this down first.

Random Highpass Filter sometimes thins out one shot. It is useful for breaking up the copy-paste sound.

Tilt EQ is a static tone tilt before generation. Positive is brighter. Negative is heavier.

Bass Reinforcement adds a low-passed copy of the shot for more punch when native filters are available.

Shot Variant Pool prebuilds a small set of shot variants and avoids playing the same variant twice in a row. I find around 6-10 works well.

Early Reflections adds short, quiet delay taps. It gives the shots a little space, but it can smear fast fire. Leave it off unless the burst sounds too dry.

Tonal Color adds subtle brightness changes per shot. It peak-matches after the color change, so it should not make random shots go quiet.

## Tuning notes

If the burst sounds choppy, lower Tail Threshold or reduce Jitter

If it gets washed out, turn off Early Reflections first

If a few shots stand out too much, reduce Gain Variation

If the cadence feels fake, try Shot Variant Pool with low Jitter
""",
        )


class LaserBeamWorkspace(ToolWorkspace):
    name = "Laser Beam Generator"
    icon = "LSR"
    id = "laser_beam"
    tool_class = LaserBeamTool
