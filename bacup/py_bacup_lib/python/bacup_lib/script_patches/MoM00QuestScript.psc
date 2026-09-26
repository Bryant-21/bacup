Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == CONST_MoM00_FoundBody
        ObjectReference corpse = MoM00Corpse.GetReference()
        If corpse == None
            Return
        EndIf

        ObjectReference damagedHolotape = MoM00Holotape.GetReference()
        If damagedHolotape != None && damagedHolotape.GetContainer() != corpse
            corpse.AddItem(damagedHolotape, 1, True)
        EndIf

        ObjectReference wornVeil = MoM00WornVeil.GetReference()
        If wornVeil != None && wornVeil.GetContainer() != corpse
            corpse.AddItem(wornVeil, 1, True)
        EndIf

        WatchCorpseItem(damagedHolotape)
        WatchCorpseItem(wornVeil)
    ElseIf auiStageID == CONST_MoM00_FinishedListeningToTheHolotape
        ; HolotapeQuest_SC sets 31 on playback. Objective 40 is shown only by stage 40 and
        ; completed by the study terminal's stage, so it must not reappear after that stage.
        MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
        If masterScript != None && !IsStageDone(CONST_CHECKPOINT_MoM00_SearchRiversideManor) && !IsStageDone(masterScript.CONST_MoM00_ReadStudyTerminal)
            SetStage(CONST_CHECKPOINT_MoM00_SearchRiversideManor)
        EndIf
    EndIf
EndEvent

Function WatchCorpseItem(ObjectReference akItem)
    If akItem == None
        Return
    EndIf
    If akItem.GetContainer() == Game.GetPlayer()
        RecordCorpseItemTaken(akItem)
    Else
        RegisterForRemoteEvent(akItem, "OnContainerChanged")
    EndIf
EndFunction

Event ObjectReference.OnContainerChanged(ObjectReference akSender, ObjectReference akNewContainer, ObjectReference akOldContainer)
    If akNewContainer == None || akNewContainer != Game.GetPlayer()
        Return
    EndIf
    UnregisterForRemoteEvent(akSender, "OnContainerChanged")
    RecordCorpseItemTaken(akSender)
EndEvent

Function RecordCorpseItemTaken(ObjectReference akItem)
    Int stage = -1
    If akItem == MoM00Holotape.GetReference()
        stage = CONST_MoM00_TookHolotape
    ElseIf akItem == MoM00WornVeil.GetReference()
        stage = CONST_MoM00_TookVeil
    EndIf
    If stage >= 0 && IsRunning() && !IsStageDone(stage)
        SetStage(stage)
    EndIf
EndFunction
