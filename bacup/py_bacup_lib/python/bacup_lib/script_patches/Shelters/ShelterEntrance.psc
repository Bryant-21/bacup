; Offline FO4 equivalent for the hollow FO76 shelter entrance. Online shelter
; ownership/recipe handling has no local analogue; the bound destination marker
; is sufficient to enter the converted interior.
;
; The return trip was server-side too, and Shelters:ShelterExit has no
; properties to hold a destination, so stamp this entrance onto the player under
; PlayerShelterEntranceKeyword for the exit to read back.

Keyword Function GetShelterEntranceLink()
    Return Game.GetFormFromFile(0x005B70F1, "SeventySix.esm") as Keyword
EndFunction

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && ShelterCellTeleportPosition != None
        Keyword entranceLink = GetShelterEntranceLink()
        If entranceLink != None
            akActionRef.SetLinkedRef(Self, entranceLink)
        EndIf
        akActionRef.MoveTo(ShelterCellTeleportPosition)
    EndIf
EndEvent
