Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = CurrentPlayer.GetActorReference()
    if playerRef == None || akActionRef != playerRef
        return
    endif

    if W05_MQS_204P_MotherlodeMSG.Show() != 1
        return
    endif

    Quest owningQuest = GetOwningQuest()
    if owningQuest == None || StageToSet <= 0 || owningQuest.IsStageDone(StageToSet)
        return
    endif

    W05_MQS_204P_QuestScript questController = owningQuest as W05_MQS_204P_QuestScript
    int finalSetting = 2
    if questController != None
        if questController.MotherlodeAggressiveChoicesMade >= 3
            finalSetting = 1
        elseif questController.MotherlodeStandardChoicesMade >= 3
            finalSetting = 2
        elseif questController.MotherlodeEmpatheticChoicesMade >= 3
            finalSetting = 3
        else
            ; The stripped client PEX exposes no mixed-value rule and the AV defines only 1-3.
            ; Use Standard for a 2-2-1 mixed result in the single-player conversion.
            finalSetting = 2
        endif
    endif
    playerRef.SetValue(W05_MQS_204P_MotherlodeFinalSetting, finalSetting as float)
    owningQuest.SetStage(StageToSet)
EndEvent
