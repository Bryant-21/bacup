; Offline FO4 survey behavior derived from the complete VMAD contract. Mark the
; activating player's per-tower actor value, notify the story manager once, and
; play the bound survey confirmation sound without relying on FO76 Sound.Play's
; network-only overload or returned instance handle.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If akActionRef.GetValue(ThisTowerValue) <= 0.0
        akActionRef.SetValue(ThisTowerValue, 1.0)
        LookoutTowerQuestKeyword.SendStoryEvent(None, Self, akActionRef)
    EndIf

    Utility.Wait(0.5)
    If SurveySound != None
        SurveySound.Play(akActionRef)
    EndIf
EndEvent
