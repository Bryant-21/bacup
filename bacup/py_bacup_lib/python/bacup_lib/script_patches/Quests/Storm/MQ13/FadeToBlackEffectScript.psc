; Hugo's capture (Storm_MQ13_FadeToBlackSpell, 10 s with the fade-to-black-and-back imagespace): the player cannot act
; while the screen is black and Hugo is moved into his captive furniture.
Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget != Game.GetPlayer()
        Return
    EndIf
    lockLayer = InputEnableLayer.Create()
    lockLayer.DisablePlayerControls(True, True, True, False, True, False, True, False, True, True, True)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If lockLayer != None
        lockLayer.EnablePlayerControls()
        lockLayer.Delete()
        lockLayer = None
    EndIf
EndEvent
