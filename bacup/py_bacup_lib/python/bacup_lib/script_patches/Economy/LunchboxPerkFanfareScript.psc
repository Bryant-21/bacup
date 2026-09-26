; FO4 has no perk-fanfare HUD, so PerkForFanfare is intentionally unused; the
; bound notification carries the feedback.

Event OnEffectStart(Actor akTarget, Actor akCaster)
    If MessageToShow && akTarget == Game.GetPlayer()
        MessageToShow.Show()
    EndIf
EndEvent
