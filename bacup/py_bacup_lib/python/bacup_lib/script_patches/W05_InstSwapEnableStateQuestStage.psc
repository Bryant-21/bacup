Event OnInit()
    Int i = 0
    Int count = EnableStates.Length
    While i < count
        If EnableStates[i].OwningQuest != None
            RegisterForRemoteEvent(EnableStates[i].OwningQuest, "OnStageSet")
        EndIf
        i += 1
    EndWhile
    EvaluateAndSetState()
EndEvent

Event OnLoad()
    EvaluateAndSetState()
EndEvent

Event Quest.OnStageSet(Quest akSender, int auiStageID, int auiItemID)
    EvaluateAndSetState()
EndEvent

Function EvaluateAndSetState()
    Bool criteriaMet = True
    If ORCriteria
        criteriaMet = False
    EndIf

    Int i = 0
    Int count = EnableStates.Length
    While i < count
        Bool entryValid = False
        If EnableStates[i].OwningQuest != None
            If EnableStates[i].TargetStageUseGetStageDone
                entryValid = EnableStates[i].OwningQuest.GetStageDone(EnableStates[i].TargetStage)
            Else
                entryValid = (EnableStates[i].OwningQuest.GetStage() == EnableStates[i].TargetStage)
            EndIf
            If EnableStates[i].ShutoffStage != -1
                If EnableStates[i].OwningQuest.GetStageDone(EnableStates[i].ShutoffStage)
                    entryValid = False
                EndIf
            EndIf
        EndIf

        If ORCriteria
            If entryValid
                criteriaMet = True
            EndIf
        Else
            If !entryValid
                criteriaMet = False
            EndIf
        EndIf

        i += 1
    EndWhile

    Bool shouldEnable = False
    If EnableObject
        shouldEnable = criteriaMet
    Else
        shouldEnable = !criteriaMet
    EndIf

    If shouldEnable
        Self.Enable()
    Else
        Self.Disable()
    EndIf

    If DejaChannel != ""
        Debug.Trace(Self as String + " W05_InstSwapEnableStateQuestStage| criteriaMet=" + criteriaMet as String + " shouldEnable=" + shouldEnable as String, 0, DejaChannel)
    EndIf
EndFunction
