Event OnQuestInit()
    ReconcileRuntimeRegistrations()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcileRuntimeRegistrations()
    EndIf
EndEvent

Event OnQuestShutdown()
    ClearRuntimeRegistrations()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == CONST_CHECKPOINT_MoM04_AllCluesFound
        ObjectReference marker = MoMBattleSiteMapMarkerRef.GetReference()
        If marker != None
            marker.AddToMap()
        EndIf
    ElseIf auiStageID == CONST_MoM04_ArrivedAtBattleSite
        PopulateBattleSiteCorpses()
    ElseIf auiStageID == CONST_CHECKPOINT_QuestCompleted
        Actor player = Game.GetPlayer()
        If player != None && MoMCryptosVoiceF_VOICEONLY != None && MoMMaster_CongratulationsTopicMistress != None
            MoMCryptosVoiceF_VOICEONLY.Say(MoMMaster_CongratulationsTopicMistress, None, False, player)
        EndIf
    EndIf
EndEvent

Function ClearRuntimeRegistrations()
    Actor player = Game.GetPlayer()
    If player != None
        UnregisterForRemoteEvent(player, "OnLocationChange")
        UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
    ObjectReference battleTrigger = MoMBattleSiteTrigger.GetReference()
    If battleTrigger != None
        UnregisterForRemoteEvent(battleTrigger, "OnTriggerEnter")
    EndIf
EndFunction

Function ReconcileRuntimeRegistrations()
    ClearRuntimeRegistrations()
    If !IsRunning() || IsCompleted()
        Return
    EndIf

    Actor player = Game.GetPlayer()
    If player != None
        RegisterForRemoteEvent(player, "OnLocationChange")
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
    ObjectReference battleTrigger = MoMBattleSiteTrigger.GetReference()
    If battleTrigger != None
        RegisterForRemoteEvent(battleTrigger, "OnTriggerEnter")
    EndIf
EndFunction

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == Game.GetPlayer() && IsRunning() && IsStageDone(CONST_CHECKPOINT_MoM04_AllCluesFound) && !IsStageDone(CONST_MoM04_ArrivedAtBattleSite) && akNewLoc == MoMBattleSiteSelectedLocation.GetLocation()
        SetStage(CONST_MoM04_ArrivedAtBattleSite)
    EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akSender == MoMBattleSiteTrigger.GetReference() && akActionRef == Game.GetPlayer() && IsRunning() && IsStageDone(CONST_CHECKPOINT_MoM04_AllCluesFound) && !IsStageDone(CONST_MoM04_ArrivedAtBattleSite)
        SetStage(CONST_MoM04_ArrivedAtBattleSite)
    EndIf
EndEvent

Function PopulateBattleSiteCorpses()
    ObjectReference shannon = ShannonRivers.GetReference()
    If shannon != None
        ObjectReference finalHolotape = FinalBattleHolotape.GetReference()
        If finalHolotape != None && finalHolotape.GetContainer() != shannon
            shannon.AddItem(finalHolotape, 1, True)
        EndIf
        ObjectReference password = ShannonRiversPassword.GetReference()
        If password != None && password.GetContainer() != shannon
            shannon.AddItem(password, 1, True)
        EndIf
    EndIf

    ObjectReference olivia = OliviaRivers.GetReference()
    ObjectReference eyeOfRa = MoMEyeOfRa.GetReference()
    If olivia != None && eyeOfRa != None && eyeOfRa.GetContainer() != olivia
        olivia.AddItem(eyeOfRa, 1, True)
    EndIf
EndFunction
