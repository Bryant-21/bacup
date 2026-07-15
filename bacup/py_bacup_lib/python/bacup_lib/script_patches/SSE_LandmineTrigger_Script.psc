Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If FragMineTick != None
        FragMineTick.Play(Self)
    EndIf
    If FragExplosion != None
        PlaceAtMe(FragExplosion)
    EndIf
    Disable()
EndEvent
