Event OnActivate(ObjectReference akActionRef)
    If hasTurnedPowerOn
        Return
    EndIf
    hasTurnedPowerOn = True
    If LC004_LightEnableMarker != None
        LC004_LightEnableMarker.Enable()
    EndIf
    If LC004_LightDisableMarker != None
        LC004_LightDisableMarker.Disable()
    EndIf
    If LC004_PowerOnSoundMarker != None && LC004_BreakerOnSound != None && LC004_ElectricityOnSound != None && LC004_BulbOnSound != None
        PlayBreakerSounds()
    EndIf
EndEvent
