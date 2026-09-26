; SCORE_Lunchbox_Level counts the distinct party-favor effects active on the
; target; the XP-bonus perk entry points are conditioned on it. The finishing
; effect may still report as active during OnEffectFinish, so the script's own
; effect is excluded from the scan and added back only while starting.

Event OnEffectStart(Actor akTarget, Actor akCaster)
    UpdateLunchboxLevel(akTarget, True)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    UpdateLunchboxLevel(akTarget, False)
EndEvent

Function UpdateLunchboxLevel(Actor akTarget, Bool abSelfActive)
    If akTarget == None || SCORE_Lunchbox_Level == None || LunchboxEffectList == None
        Return
    EndIf

    MagicEffect selfEffect = GetBaseObject()
    Int level = 0
    If abSelfActive && LunchboxEffectList.HasForm(selfEffect)
        level = 1
    EndIf

    Int i = 0
    Int size = LunchboxEffectList.GetSize()
    While i < size
        MagicEffect listed = LunchboxEffectList.GetAt(i) as MagicEffect
        If listed && listed != selfEffect && akTarget.HasMagicEffect(listed)
            level += 1
        EndIf
        i += 1
    EndWhile

    akTarget.SetValue(SCORE_Lunchbox_Level, level as Float)
EndFunction
