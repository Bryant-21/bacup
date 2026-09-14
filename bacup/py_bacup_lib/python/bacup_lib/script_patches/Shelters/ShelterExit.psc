; FO76 ran the way out of a shelter server-side, so the shipped client script is
; a bodiless stub with no properties at all — the "Exit to Appalachia" prompt
; fires OnActivate and nothing happens.
;
; Shelters:ShelterEntrance stamps the entrance the player walked in through onto
; the player under PlayerShelterEntranceKeyword. Read it back here and return
; them to it; without that stamp there is no way to know which of the shared
; shelter interior's entrances to send them out through.

Keyword Function GetShelterEntranceLink()
    Return Game.GetFormFromFile(0x005B70F1, "SeventySix.esm") as Keyword
EndFunction

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    Keyword entranceLink = GetShelterEntranceLink()
    If entranceLink == None
        Return
    EndIf

    ObjectReference entrance = akActionRef.GetLinkedRef(entranceLink)
    If entrance != None
        akActionRef.MoveTo(entrance, abMatchRotation = False)
    EndIf
EndEvent
