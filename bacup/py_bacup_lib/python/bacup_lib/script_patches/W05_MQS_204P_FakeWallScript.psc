Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = CurrentPlayer.GetActorReference()
    Quest owningQuest = GetOwningQuest()
    If playerRef == None || akActionRef != playerRef || owningQuest == None || StageToSet <= 0 || owningQuest.IsStageDone(StageToSet)
        Return
    EndIf

    Int selectedAction = W05_MQS_204P_FakeWallMSG.Show()
    If selectedAction == 0
        Return
    EndIf

    W05_MQS_204P_QuestScript questController = owningQuest as W05_MQS_204P_QuestScript
    If questController == None
        Return
    EndIf

    If selectedAction == 1
        questController.MotherlodeAggressiveChoicesMade = questController.MotherlodeAggressiveChoicesMade + 1
        W05_MQS_204P_FakeWallMSG_Aggressive.Show()
        If W05_MQS_204P_006A_AggressionScene != None && !W05_MQS_204P_006A_AggressionScene.IsPlaying()
            W05_MQS_204P_006A_AggressionScene.Start()
        EndIf
    ElseIf selectedAction == 2
        questController.MotherlodeStandardChoicesMade = questController.MotherlodeStandardChoicesMade + 1
        W05_MQS_204P_FakeWallMSG_Standard.Show()
        If W05_MQS_204P_006B_StandardSetting != None && !W05_MQS_204P_006B_StandardSetting.IsPlaying()
            W05_MQS_204P_006B_StandardSetting.Start()
        EndIf
    ElseIf selectedAction == 3
        questController.MotherlodeEmpatheticChoicesMade = questController.MotherlodeEmpatheticChoicesMade + 1
        ; Each bound wall stage has one corresponding numbered response.
        If StageToSet == 610 && W05_MQS_204P_FakeWallMSG_Empathetic01 != None
            W05_MQS_204P_FakeWallMSG_Empathetic01.Show()
        ElseIf StageToSet == 620 && W05_MQS_204P_FakeWallMSG_Empathetic02 != None
            W05_MQS_204P_FakeWallMSG_Empathetic02.Show()
        ElseIf StageToSet == 630 && W05_MQS_204P_FakeWallMSG_Empathetic03 != None
            W05_MQS_204P_FakeWallMSG_Empathetic03.Show()
        ElseIf StageToSet == 640 && W05_MQS_204P_FakeWallMSG_Empathetic04 != None
            W05_MQS_204P_FakeWallMSG_Empathetic04.Show()
        ElseIf StageToSet == 650 && W05_MQS_204P_FakeWallMSG_Empathetic05 != None
            W05_MQS_204P_FakeWallMSG_Empathetic05.Show()
        EndIf
        If W05_MQS_204P_006C_EmpatheticScene != None && !W05_MQS_204P_006C_EmpatheticScene.IsPlaying()
            W05_MQS_204P_006C_EmpatheticScene.Start()
        EndIf
    Else
        Return
    EndIf

    owningQuest.SetStage(StageToSet)
EndEvent
