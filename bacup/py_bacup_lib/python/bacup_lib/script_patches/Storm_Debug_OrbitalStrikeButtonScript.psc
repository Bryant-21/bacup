Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || OrbitalStrikeExplosion == None
        Return
    EndIf

    ObjectReference strikeRef = PlaceAtMe(OrbitalStrikeExplosion)
    If strikeRef != None && fExplosionSize > 0.0
        strikeRef.SetScale(fExplosionSize)
    EndIf
EndEvent
