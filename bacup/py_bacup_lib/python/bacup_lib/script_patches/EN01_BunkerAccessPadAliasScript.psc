Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If EN01_MQ_Bunker.IsStageDone(40) || playerRef.GetValue(EN01_PlayerTriggeredBypassValue) >= iBypassValue as Float
        OpenBunkerDoor()
        Return
    EndIf
    If !EN01_MQ_Bunker.IsStageDone(5)
        EN01_MQ_Bunker.SetStage(5)
    EndIf
    If !bTriggerDialogue
        bTriggerDialogue = True
        ObjectReference voiceRef = GenericMachineVoice.GetRef()
        If voiceRef != None
            voiceRef.Say(EN01_BunkerAccessDenied, akTarget = playerRef)
        EndIf
        StartTimer(iTimerLength as Float, iTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iTimerID
        bTriggerDialogue = False
    EndIf
EndEvent

Function OpenBunkerDoor()
    ObjectReference doorRef = InvisibleBunkerDoor
    If doorRef == None
        doorRef = Game.GetFormFromFile(0x001AE92D, "SeventySix.esm") as ObjectReference
    EndIf
    If doorRef != None
        doorRef.Lock(False)
        doorRef.SetOpen(True)
        doorRef.Activate(Game.GetPlayer())
    EndIf
EndFunction
