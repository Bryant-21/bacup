Event OnActivate(ObjectReference akActionRef)
    If ExplosionToPlay == None
        Return
    EndIf

    If fExplosionDelay > 0.0
        StartTimer(fExplosionDelay, iExplosionDelayTimerID)
    Else
        PlayExplosion()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iExplosionDelayTimerID
        PlayExplosion()
    EndIf
EndEvent

Function PlayExplosion()
    If ExplosionToPlay == None
        Return
    EndIf

    If NodeToPlayExplosion != ""
        PlaceAtNode(NodeToPlayExplosion, ExplosionToPlay)
    Else
        PlaceAtMe(ExplosionToPlay)
    EndIf
EndFunction
