Function Fragment_End(Actor akActor)
    ; The street Billy hands off to the separate BillyNightClub actor enabled inside Quentino's.
    If BillyAlias == None
        Return
    EndIf
    Actor billyRef = BillyAlias.GetActorReference()
    If billyRef != None && billyRef == akActor
        billyRef.DisableNoWait()
    EndIf
EndFunction
