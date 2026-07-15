; Offline FO4 equivalent for the hollow FO76 candy bowl. CAMP ownership,
; cooldown, costume, challenge, and shared bowl-inventory semantics are online
; systems, so a player activation grants one bound candy and reports success.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        akActionRef.AddItem(HalloweenCandy, 1, False)
        TrickOrTreatSuccessMessage.Show()
    EndIf
EndEvent
