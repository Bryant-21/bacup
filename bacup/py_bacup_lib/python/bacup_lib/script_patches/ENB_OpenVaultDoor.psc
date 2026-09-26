Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || player == None || EN02_MQ_Us == None
        Return
    EndIf
    If !EN02_MQ_Us.IsStageDone(12) && !EN02_MQ_Us.IsCompleted()
        PlayFailureSoundEffect()
        Return
    EndIf
    LC080_WhitespringBunkerGearDoorScript gearDoor = GetLinkedRef() as LC080_WhitespringBunkerGearDoorScript
    If gearDoor != None
        gearDoor.SetOpen(True)
        PlaySuccessSoundEffect()
    EndIf
EndEvent

Function PlaySuccessSoundEffect()
    If OBJVaultGearPanelSuccess != None
        OBJVaultGearPanelSuccess.Play(Self)
    EndIf
EndFunction

Function PlayFailureSoundEffect()
    If OBJVaultGearPanelFail != None
        OBJVaultGearPanelFail.Play(Self)
    EndIf
EndFunction
