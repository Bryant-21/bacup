Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    ObjectReference flareOrigin = GetLinkedRef(FlareOriginKeyword)
    ObjectReference flareTarget = GetLinkedRef(FlareTargetKeyword)
    If flareOrigin == None
        flareOrigin = Self as ObjectReference
    EndIf

    If ExplosionElectricalSmall != None
        flareOrigin.PlaceAtMe(ExplosionElectricalSmall)
    EndIf
    If Bounty_FlareSpell != None && flareTarget != None
        Bounty_FlareSpell.Cast(flareOrigin, flareTarget)
    EndIf
EndEvent
