Event OnSit(ObjectReference akFurniture)
    ObjectReference scannerRef = BrainwaveScanner.GetReference()
    Quest owningQuest = GetOwningQuest()
    If scannerRef == None || akFurniture != scannerRef || owningQuest == None || StageToSet <= 0 || owningQuest.IsStageDone(StageToSet)
        Return
    EndIf

    W05_MQS_204P_QuestScript questController = owningQuest as W05_MQS_204P_QuestScript
    If questController == None
        Return
    EndIf

    Scene resultScene
    ; Five walls produce one choice each; three is the only non-mixed majority.
    If questController.MotherlodeAggressiveChoicesMade >= 3
        resultScene = W05_MQS_204P_008A_AggressiveResults
    ElseIf questController.MotherlodeStandardChoicesMade >= 3
        resultScene = W05_MQS_204P_008B_StandardResults
    ElseIf questController.MotherlodeEmpatheticChoicesMade >= 3
        resultScene = W05_MQS_204P_008C_EmpatheticResults
    Else
        resultScene = W05_MQS_204P_008D_MixedResults
    EndIf

    If resultScene != None && !resultScene.IsPlaying()
        resultScene.Start()
    EndIf
    owningQuest.SetStage(StageToSet)
EndEvent
