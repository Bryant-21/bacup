Actor Function GetVertibot()
    If Alias_AirDropVertibot == None
        Return None
    EndIf
    Return Alias_AirDropVertibot.GetActorReference()
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If DropStartStage >= 0 && auiStageID == DropStartStage
        StartAirDrop()
    EndIf
EndEvent

Function StartAirDrop()
    If containerDropped
        Return
    EndIf
    Actor vertibot = GetVertibot()
    If vertibot == None
        ; Without a Cargobot reference the supply drop still has to arrive.
        DropCargo()
        Return
    EndIf
    If vertibot.IsDisabled()
        vertibot.Enable(False)
    EndIf
    RegisterForRemoteEvent(vertibot, "OnDeath")
    If FF11_Raid_VertibotScene != None && !FF11_Raid_VertibotScene.IsPlaying()
        FF11_Raid_VertibotScene.Start()
    EndIf
    ; The scene's phase-1 fragment lands the Cargobot and drops the cargo; this
    ; covers a scene that never plays or never reaches its second phase.
    CancelTimer(1167)
    StartTimer(60.0, 1167)
EndFunction

Function DropCargo()
    If containerDropped
        Return
    EndIf
    ObjectReference dropRef = None
    If Alias_DropTarget != None
        dropRef = Alias_DropTarget.GetReference()
    EndIf
    If dropRef == None && Alias_VertibotSpawnPoint != None
        dropRef = Alias_VertibotSpawnPoint.GetReference()
    EndIf
    If dropRef == None || AirDropContainer == None
        Return
    EndIf
    containerDropped = True
    CancelTimer(1167)
    dropRef.PlaceAtMe(AirDropContainer, 1, True, False, True)
    If DropCompleteStage >= 0 && !IsStageDone(DropCompleteStage)
        SetStage(DropCompleteStage)
    EndIf
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender != GetVertibot()
        Return
    EndIf
    UnregisterForRemoteEvent(akSender, "OnDeath")
    If VertibotDeadStage >= 0 && !IsStageDone(VertibotDeadStage)
        SetStage(VertibotDeadStage)
    EndIf
    If stopQuestOnDeath && IsRunning()
        Stop()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1167 || containerDropped || !IsRunning()
        Return
    EndIf
    Actor vertibot = GetVertibot()
    If vertibot != None && VertibirdLand != None
        vertibot.SetValue(VertibirdLand, 1.0)
        vertibot.EvaluatePackage()
    EndIf
    DropCargo()
EndEvent

Event OnQuestInit()
    containerDropped = False
EndEvent

Event OnQuestShutdown()
    CancelTimer(1167)
    UnregisterForAllEvents()
EndEvent
