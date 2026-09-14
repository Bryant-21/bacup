Event OnHolotapePlay(ObjectReference akTerminalRef)
    TryBypass()
EndEvent

Function TryBypass()
    Quest bunkerQuest = GetOwningQuest()
    Actor playerRef = Game.GetPlayer()
    ObjectReference triggerRef = BypassTapeTrigger.GetRef()
    If bunkerQuest == None || playerRef == None || bunkerQuest.IsStageDone(iStageToSet)
        Return
    EndIf
    If !bunkerQuest.IsStageDone(30) || playerRef.GetItemCount(EN01_BypassHolotape) <= 0
        Return
    EndIf
    If triggerRef != None && playerRef.GetDistance(triggerRef) > 1024.0
        Return
    EndIf
    bunkerQuest.SetStage(iStageToSet)
    ObjectReference accessPad = BunkerDoorAccess.GetRef()
    If accessPad != None
        accessPad.Activate(playerRef)
    EndIf
EndFunction
