; Offline FO4 equivalent for the hollow FO76 shelter entrance. Online shelter
; ownership/recipe handling has no local analogue; the bound destination marker
; is sufficient to enter the converted interior.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && ShelterCellTeleportPosition != None
        akActionRef.MoveTo(ShelterCellTeleportPosition)
    EndIf
EndEvent
