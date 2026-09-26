Event OnQuestInit()
    Quest owner = Self as Quest
    ProgressBar = owner as Quests:_Default:ProgressBar:MasterScript
EndEvent

Event OnQuestShutdown()
    CancelTimer(7331)
    StopIntercomWait()
    ClearPartDispensers()
    RemoveHarvesterPartsFromPlayer()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7331
        CollectChargedKills()
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || !IsRunning() || IsStageDone(100)
        Return
    EndIf
    StopIntercomWait()
    SetStage(100)
EndEvent

ObjectReference Function GetIntercom()
    ; Alias 15 is Activator_David_Intercom; the scripts carry no property for it.
    ReferenceAlias intercomAlias = GetAlias(15) as ReferenceAlias
    If intercomAlias == None
        Return None
    EndIf
    Return intercomAlias.GetReference()
EndFunction

Function BeginIntercomWait()
    ObjectReference intercom = GetIntercom()
    If intercom != None
        RegisterForRemoteEvent(intercom, "OnActivate")
    EndIf
EndFunction

Function StopIntercomWait()
    ObjectReference intercom = GetIntercom()
    If intercom != None
        UnregisterForRemoteEvent(intercom, "OnActivate")
    EndIf
EndFunction

Quests:_Default:ProgressBar:MasterScript Function ResolveProgressBar()
    If ProgressBar == None
        Quest owner = Self as Quest
        ProgressBar = owner as Quests:_Default:ProgressBar:MasterScript
    EndIf
    Return ProgressBar
EndFunction

Function SetInstalledPartCount(Float afCount)
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable("HarvesterPartsCollected", afCount)
    EndIf
EndFunction

Function BeginPartCollection()
    SetInstalledPartCount(0.0)
    ; Alias 14 is HarvesterInstallTriggers; its install count persists across quest restarts.
    Storm_E01_PartsInstallScript installTriggers = GetAlias(14) as Storm_E01_PartsInstallScript
    If installTriggers != None
        installTriggers.ResetInstalledParts()
    EndIf
    CreatePartDispensers()
EndFunction

Function EndPartCollection()
    ClearPartDispensers()
    RemoveHarvesterPartsFromPlayer()
EndFunction

Function CreatePartDispensers()
    If HarvesterPartsDispenserLocationMarkers == None || HarvesterPartsDispensers == None || Storm_E01_Dangerous_HarvesterParts == None
        Return
    EndIf
    ClearPartDispensers()
    Int index = 0
    While index < HarvesterPartsDispenserLocationMarkers.GetCount()
        ObjectReference marker = HarvesterPartsDispenserLocationMarkers.GetAt(index)
        If marker != None
            ObjectReference part = marker.PlaceAtMe(Storm_E01_Dangerous_HarvesterParts, 1, False, False, True)
            If part != None
                HarvesterPartsDispensers.AddRef(part)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function ClearPartDispensers()
    If HarvesterPartsDispensers == None
        Return
    EndIf
    Int index = HarvesterPartsDispensers.GetCount() - 1
    While index >= 0
        ObjectReference part = HarvesterPartsDispensers.GetAt(index)
        If part != None
            HarvesterPartsDispensers.RemoveRef(part)
            ; A part already in an inventory has no world ref to delete.
            If part.GetContainer() == None
                part.Delete()
            EndIf
        EndIf
        index -= 1
    EndWhile
EndFunction

Function RemoveHarvesterPartsFromPlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || Storm_E01_Dangerous_HarvesterParts == None
        Return
    EndIf
    Int count = playerRef.GetItemCount(Storm_E01_Dangerous_HarvesterParts)
    If count > 0
        playerRef.RemoveItem(Storm_E01_Dangerous_HarvesterParts, count, True)
    EndIf
EndFunction

Function StartChargeTracking()
    Quests:_Default:ProgressBar:MasterScript bar = ResolveProgressBar()
    If bar != None
        bar.SetProgress(0.0)
        bar.DisplayProgressBar()
    EndIf
    StartTimer(2.0, 7331)
EndFunction

Function StopChargeTracking()
    CancelTimer(7331)
    Quests:_Default:ProgressBar:MasterScript bar = ResolveProgressBar()
    If bar != None
        bar.HideProgressBar()
    EndIf
EndFunction

Function CollectChargedKills()
    If !IsRunning() || !IsStageDone(iHarvesterPartsCollectionStageToSetOnComplete) || IsStageDone(365)
        Return
    EndIf
    Quests:_Default:ProgressBar:MasterScript bar = ResolveProgressBar()
    Int index = 0
    If ChargedMobs != None
        index = ChargedMobs.GetCount() - 1
    EndIf
    While ChargedMobs != None && index >= 0
        Actor chargedActor = ChargedMobs.GetAt(index) as Actor
        If chargedActor == None
            ChargedMobs.RemoveRef(ChargedMobs.GetAt(index))
        ElseIf chargedActor.IsDead()
            ChargedMobs.RemoveRef(chargedActor)
            If bar != None
                bar.ModProgress(ChargedKillProgress)
            EndIf
        EndIf
        index -= 1
    EndWhile
    StartTimer(2.0, 7331)
EndFunction
