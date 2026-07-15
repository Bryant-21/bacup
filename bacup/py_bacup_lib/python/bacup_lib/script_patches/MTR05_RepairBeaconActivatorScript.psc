Event OnActivate(ObjectReference akActionRef)
    If bActive
        Return
    EndIf

    bActive = True
    BuildUpSound.Play(Self)
    If iTimeToWait > 0
        Utility.Wait(iTimeToWait)
    EndIf
    PlaceAtMe(MTR05_BeaconLaunchExplosion)
    bActive = False
EndEvent
