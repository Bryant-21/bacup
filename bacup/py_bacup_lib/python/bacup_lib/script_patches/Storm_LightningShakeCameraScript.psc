Event OnEffectStart(Actor akTarget, Actor akCaster)
    Float afDuration
    Game.ShakeCamera(akCaster as ObjectReference, CameraShakeIntensity, afDuration)
    Game.ShakeController(ControllerShakeIntensity, ControllerShakeIntensity, afDuration)
EndEvent
